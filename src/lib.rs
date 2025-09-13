use image::{ImageBuffer, Rgba};
use std::collections::HashSet;
use piston_window::{
    clear, image::Image, Button, EventLoop, Filter, Flip, FocusEvent, G2dTexture, G2dTextureContext, Key, PistonWindow, PressEvent, ReleaseEvent, Texture, TextureSettings, UpdateEvent, Window, WindowSettings};
use std::cmp;

/// -------- Engine constants (change to taste) --------
const LOW_W: u32 = 320;
const LOW_H: u32 = 200; // 16:9 pixel canvas
const FIXED_DT: f64 = 1.0 / 60.0;

pub fn div_to_floor(a: i32, b: i32) -> i32 {
    let mut r = a / b;
    if (a ^ b) < 0 && a % b != 0 { r -= 1; }
    r
}

#[inline]
fn sprite_visible(sx: i32, sy: i32, sw: i32, sh: i32, vw: i32, vh: i32) -> bool {
    // rect (sx..sx+sw, sy..sy+sh) intersects (0..vw, 0..vh)
    !(sx >= vw || sy >= vh || sx + sw <= 0 || sy + sh <= 0)
}

pub struct Camera {
    pub x: f32,
    pub y: f32,
    pub viewport_w: u32,
    pub viewport_h: u32,
    pub zoom: f32,
}

impl Camera {
    fn new() -> Self {
        Self { x: 0.0, y: 0.0, viewport_w: LOW_W, viewport_h: LOW_H, zoom: 1.0, }
    }

    #[inline]
    pub fn world_to_screen(&self, wx: f32, wy: f32) -> (i32, i32) {
        let sx = ((wx - self.x) * self.zoom).floor() as i32;
        let sy = ((wy - self.y) * self.zoom).floor() as i32;
        (sx, sy)
    }

    pub fn screen_to_world(&self, sx: i32, sy: i32) -> (f32, f32) {
        let wx = (sx as f32) / self.zoom + self.x;
        let wy = (sy as f32) / self.zoom + self.y;
        (wx, wy)
    }

    pub fn visible_tiles(&self, tile_size: u32) -> (i32, i32, i32, i32) {
        let x0 = (self.x / tile_size as f32).floor() as i32;
        let y0 = (self.y / tile_size as f32).floor() as i32;

        let tiles_x = (self.viewport_w / tile_size) as i32 + 2; // cover edges
        let tiles_y = (self.viewport_h / tile_size) as i32 + 2;

        let x1 = x0 + tiles_x;
        let y1 = y0 + tiles_y;

        (x0, y0, x1, y1)
    }
}

pub struct Assets<'a> {
    tex_ctx: &'a mut G2dTextureContext,
}

impl<'a> Assets<'a> {
    pub fn load_image(&mut self, path: &str) -> image::RgbaImage {
        image::open(path).expect("img").to_rgba8()
    }
    pub fn load_texture(&mut self, path: &str) -> G2dTexture {
        Texture::from_path(
            self.tex_ctx, path, Flip::None,
            &TextureSettings::new().filter(Filter::Nearest),
        ).expect("tex")
    }
    // later: load_sound, load_font, etc.
}

/// -------- Render helpers --------
pub fn make_nearest_texture(tc: &mut G2dTextureContext, buf: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> G2dTexture {
    let mut ts = TextureSettings::new();
    ts.set_filter(Filter::Nearest); // pixel crisp
    Texture::from_image(tc, buf, &ts).expect("texture")
}

/// -------- PixelBuffer: your CPU-side framebuffer --------
#[derive(Clone)]
pub struct PixelBuffer {
    w: u32,
    h: u32,
    buf: ImageBuffer<Rgba<u8>, Vec<u8>>,
}
impl crate::PixelBuffer {
    pub fn new(w: u32, h: u32) -> Self {
        let buf = ImageBuffer::from_pixel(w, h, Rgba([0, 0, 0, 255]));
        Self { w, h, buf }
    }
    #[inline] pub fn width(&self) -> u32 { self.w }
    #[inline] pub fn height(&self) -> u32 { self.h }
    /// Clear to RGBA
    pub fn clear(&mut self, color: [u8; 4]) {
        // Fast bulk clear: fill + fix alpha if needed
        if color == [0, 0, 0, 255] {
            self.buf.as_mut().fill(0);
            for p in self.buf.pixels_mut() { p.0[3] = 255; }
        } else {
            for p in self.buf.pixels_mut() { *p = Rgba(color); }
        }
    }
    /// Safe pixel plot (clamped)
    pub fn put(&mut self, x: i32, y: i32, c: [u8; 4]) {
        if x >= 0 && y >= 0 && (x as u32) < self.w && (y as u32) < self.h {
            self.buf.put_pixel(x as u32, y as u32, Rgba(c));
        }
    }
    /// Bresenham line
    pub fn line(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, c: [u8; 4]) {
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

    pub fn blit_to_camera(&mut self, cam: &Camera, sprite_w: u32, sprite_h: u32, pixels: &[[u8; 4]], world_x: f32, world_y: f32) {
      let (sx, sy) = cam.world_to_screen(world_x, world_y);

      let sw = (sprite_w as f32 * cam.zoom).ceil() as i32;  // scale if zoom != 1
      let sh = (sprite_h as f32 * cam.zoom).ceil() as i32;

      let vw = cam.viewport_w as i32;
      let vh = cam.viewport_h as i32;

      if !sprite_visible(sx, sy, sw, sh, vw, vh) {
        return; // fully off-screen, skip
      }

       // If zoom == 1.0, call your unscaled blit.
       // <to support zoom, route to a scaled blitter.
       self.blit_rgba(sx, sy, sprite_w, sprite_h, pixels);
    }

    /// Alpha-blit a small sprite buffer (premult not required; simple over)
    pub fn blit_rgba(
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
pub trait Scene {
    fn update(&mut self, dt: f64, fb: &mut crate::PixelBuffer);
    fn draw(&self, fb: &mut crate::PixelBuffer);
    fn key_event(&mut self, _key: Key, _down: bool) { } // optional
    fn on_load(&mut self, _assets: &mut Assets) {} // once
}

pub struct PixEngine {
    window: PistonWindow,
    scene: Box<dyn Scene>,
    framebuffer: PixelBuffer,
    tex_ctx: G2dTextureContext,
    tex: G2dTexture,
    pressed: HashSet<Key>,
}

impl PixEngine {
    pub fn new(window_width: u32, window_height: u32, fullscreen: bool, window_title: &str, mut scene: impl Scene + 'static ) -> Self {
        let mut window: PistonWindow = WindowSettings::new(window_title, [window_width, window_height])
            .exit_on_esc(true)
            .fullscreen(fullscreen)
            .build()
            .unwrap();
        window.set_ups(120);   // high logical UPS for smooth physics
        window.set_max_fps(60);
        let fb = PixelBuffer::new(LOW_W, LOW_H);
        let mut tex_ctx = window.create_texture_context();
        let tex = make_nearest_texture( & mut tex_ctx, & fb.buf);
        let pressed = HashSet::new();
        // Give the game a chance to load assets safely (no double &mut)
        {
            let mut assets = Assets { tex_ctx: &mut tex_ctx };
            scene.on_load(&mut assets);
        }
        Self { window, scene: Box::new(scene), framebuffer: fb, tex_ctx, tex, pressed }
       
    }
    
    pub fn load_sprite_atlas(&mut self, path: &str) -> G2dTexture {
        Texture::from_path(
            &mut self.tex_ctx,
            path,
            Flip::None,
            &TextureSettings::new().filter(Filter::Nearest),
       ).expect("Sprite atlas loading failed!")
    }

    pub fn run(&mut self) {
        let mut acc = 0.0;

        let [win_w, win_h]: [u32; 2] = self.window.size().into();


        while let Some(e) = self.window.next() {
            if let Some(btn) = e.press_args() {
                if let Button::Keyboard(k) = btn {
                    // Ignore key-repeat: insert returns false if it was already down
                    if self.pressed.insert(k) {
                        // scene key-down callback (optional)
                        self.scene.key_event(k, true);
                    }
                }
            }
            if let Some(btn) = e.release_args() {
                if let Button::Keyboard(k) = btn {
                    if self.pressed.remove(&k) {
                        // scene key-up callback (optional)
                        self.scene.key_event(k, false);
                    }
                }
            }

            // --- If window loses focus, clear keys to avoid “stuck key” bugs
            if let Some(focused) = e.focus_args() {
                if !focused { self.pressed.clear(); }
            }


            if let Some(u) = e.update_args() {
                acc += u.dt;
                while acc >= FIXED_DT {
                    self.scene.update(FIXED_DT, & mut self.framebuffer);
                    acc -= FIXED_DT;
                }
                // draw into pixel buffer
                self.scene.draw( & mut self.framebuffer);
                // upload CPU → GPU
                self.tex.update( & mut self.tex_ctx, & self.framebuffer.buf).unwrap();
            }

            self.window.draw_2d( & e, | c, g, device | {
                // flush pending texture updates
                self.tex_ctx.encoder.flush(device);

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
                    .draw( & self.tex, & c.draw_state, c.transform, g);
            });
        }
    }

}
