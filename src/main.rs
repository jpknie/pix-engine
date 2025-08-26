use image::{ImageBuffer, Rgba};
use piston_window::{clear, image::Image, EventLoop, Filter, G2d, G2dTexture, G2dTextureContext, PistonWindow, Texture, TextureSettings, UpdateEvent, Window, WindowSettings};
use std::cmp;

/// -------- Engine constants (change to taste) --------
const LOW_W: u32 = 320;
const LOW_H: u32 = 180; // 16:9 pixel canvas
const FIXED_DT: f64 = 1.0 / 60.0;

/// -------- PixelBuffer: your CPU-side framebuffer --------
#[derive(Clone)]
struct PixelBuffer {
    w: u32,
    h: u32,
    buf: ImageBuffer<Rgba<u8>, Vec<u8>>,
}
impl PixelBuffer {
    fn new(w: u32, h: u32) -> Self {
        let buf = ImageBuffer::from_pixel(w, h, Rgba([0, 0, 0, 255]));
        Self { w, h, buf }
    }
    #[inline] fn width(&self) -> u32 { self.w }
    #[inline] fn height(&self) -> u32 { self.h }
    /// Clear to RGBA
    fn clear(&mut self, color: [u8; 4]) {
        // Fast bulk clear: fill + fix alpha if needed
        if color == [0, 0, 0, 255] {
            self.buf.as_mut().fill(0);
            for p in self.buf.pixels_mut() { p.0[3] = 255; }
        } else {
            for p in self.buf.pixels_mut() { *p = Rgba(color); }
        }
    }
    /// Safe pixel plot (clamped)
    fn put(&mut self, x: i32, y: i32, c: [u8; 4]) {
        if x >= 0 && y >= 0 && (x as u32) < self.w && (y as u32) < self.h {
            self.buf.put_pixel(x as u32, y as u32, Rgba(c));
        }
    }
    /// Bresenham line
    fn line(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, c: [u8; 4]) {
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.put(x0, y0, c);
            if x0 == x1 && y0 == y1 { break; }
            let e2 = 2 * err;
            if e2 >= dy { err += dy; x0 += sx; }
            if e2 <= dx { err += dx; y0 += sy; }
        }
    }
    /// Alpha-blit a small sprite buffer (premult not required; simple over)
    fn blit_rgba(
        &mut self,
        sx: i32,
        sy: i32,
        sprite_w: u32,
        sprite_h: u32,
        pixels: &[[u8; 4]],
    ) {
        let sw = sprite_w as i32;
        let sh = sprite_h as i32;
        for j in 0..sh {
            for i in 0..sw {
                let px = i + sx;
                let py = j + sy;
                if px < 0 || py < 0 || (px as u32) >= self.w || (py as u32) >= self.h { continue; }
                let s = pixels[(j as usize) * sprite_w as usize + i as usize];
                let a = s[3] as f32 / 255.0;
                if a <= 0.0 { continue; }
                let dst = self.buf.get_pixel(px as u32, py as u32).0;
                let out = [
                    (s[0] as f32 * a + dst[0] as f32 * (1.0 - a)) as u8,
                    (s[1] as f32 * a + dst[1] as f32 * (1.0 - a)) as u8,
                    (s[2] as f32 * a + dst[2] as f32 * (1.0 - a)) as u8,
                    255,
                ];
                self.buf.put_pixel(px as u32, py as u32, Rgba(out));
            }
        }
    }
}

/// -------- Scene trait: plug in your game/effect --------
trait Scene {
    fn update(&mut self, dt: f64, fb: &mut PixelBuffer);
    fn draw(&self, fb: &mut PixelBuffer);
}

/// -------- Example scene: moving sprite + lines --------
struct DemoScene {
    t: f64,
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    sprite: Vec<[u8; 4]>,
    sw: u32,
    sh: u32,
}
impl DemoScene {
    fn new() -> Self {
        // Tiny 8x8 checker sprite with alpha
        let sw = 8;
        let sh = 8;
        let mut sprite = vec![[0, 0, 0, 0]; (sw * sh) as usize];
        for j in 0..sh {
            for i in 0..sw {
                let on = ((i + j) % 2) == 0;
                sprite[(j * sw + i) as usize] = if on { [230, 120, 255, 255] } else { [40, 10, 60, 255] };
            }
        }
        Self { t: 0.0, x: 10.0, y: 20.0, vx: 32.0, vy: 20.0, sprite, sw, sh }
    }
}
impl Scene for DemoScene {
    fn update(&mut self, dt: f64, fb: &mut PixelBuffer) {
        self.t += dt;
        // simple bounce
        self.x += self.vx * dt;
        self.y += self.vy * dt + (self.t * 2.0).sin() * 8.0 * dt;
        if self.x < 0.0 || self.x > (fb.width() - self.sw) as f64 { self.vx = -self.vx; }
        if self.y < 0.0 || self.y > (fb.height() - self.sh) as f64 { self.vy = -self.vy; }
    }
    fn draw(&self, fb: &mut PixelBuffer) {
        fb.clear([8, 8, 10, 255]);
        // crosshair lines
        fb.line(0, (fb.height() / 2) as i32, (fb.width() - 1) as i32, (fb.height() / 2) as i32, [30, 30, 50, 255]);
        fb.line((fb.width() / 2) as i32, 0, (fb.width() / 2) as i32, (fb.height() - 1) as i32, [30, 30, 50, 255]);
        // blit sprite
        self.blit(fb);
    }
}
impl DemoScene {
    fn blit(&self, fb: &mut PixelBuffer) {
        fb.blit_rgba(self.x as i32, self.y as i32, self.sw, self.sh, &self.sprite);
    }
}

/// -------- Render helpers --------
fn make_nearest_texture(tc: &mut G2dTextureContext, buf: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> G2dTexture {
    let mut ts = TextureSettings::new();
    ts.set_filter(Filter::Nearest); // pixel crisp
    Texture::from_image(tc, buf, &ts).expect("texture")
}

fn main() {
    let mut window: PistonWindow = WindowSettings::new("Pixel Engine (Piston)", [1280, 720])
        .exit_on_esc(true)
        .build()
        .unwrap();

    window.set_ups(120);   // high logical UPS for smooth physics
    window.set_max_fps(60);

    let mut tex_ctx = window.create_texture_context();

    // Low-res pixel canvas + GPU texture
    let mut fb = PixelBuffer::new(LOW_W, LOW_H);
    let mut tex = make_nearest_texture(&mut tex_ctx, &fb.buf);

    // Your scene
    let mut scene = DemoScene::new();

    // Fixed timestep accumulator
    let mut acc = 0.0;

    let [win_w, win_h]: [u32; 2] = window.size().into();


    while let Some(e) = window.next() {
        if let Some(u) = e.update_args() {
            acc += u.dt;
            while acc >= FIXED_DT {
                scene.update(FIXED_DT, &mut fb);
                acc -= FIXED_DT;
            }
            // draw into pixel buffer
            scene.draw(&mut fb);
            // upload CPU → GPU
            tex.update(&mut tex_ctx, &fb.buf).unwrap();
        }

        window.draw_2d(&e, |c, g, device| {
            // flush pending texture updates
            tex_ctx.encoder.flush(device);

            // clear the window framebuffer
            clear([0.07, 0.07, 0.08, 1.0], g);

            // integer upscale to keep pixels crisp
            //let [win_w, win_h]: [u32; 2] = e.draw_size().into();
            let sx = win_w / LOW_W;
            let sy = win_h / LOW_H;
            let scale = cmp::min(sx, sy).max(1);
            let draw_w = (LOW_W * scale) as f64;
            let draw_h = (LOW_H * scale) as f64;
            let off_x = ((win_w as f64 - draw_w) * 0.5).floor();
            let off_y = ((win_h as f64 - draw_h) * 0.5).floor();

            Image::new()
                .rect([off_x, off_y, draw_w, draw_h])
                .draw(&tex, &c.draw_state, c.transform, g);
        });
    }
}
