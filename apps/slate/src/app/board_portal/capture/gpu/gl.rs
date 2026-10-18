//! GL resources and state restoration for the shared rendered-content seam.

use eframe::glow;
use glow::HasContext as _;

use super::geometry::{Plan, Size};

pub(super) struct Restore {
    read: Option<glow::Framebuffer>,
    draw: Option<glow::Framebuffer>,
    pack: Option<glow::Buffer>,
    renderbuffer: Option<glow::Renderbuffer>,
    alignment: i32,
    row_length: i32,
    skip_rows: i32,
    skip_pixels: i32,
    scissor: bool,
    scissor_box: [i32; 4],
    srgb: bool,
    clear: [f32; 4],
}

impl Restore {
    pub(super) unsafe fn save(gl: &glow::Context) -> Self {
        let mut scissor_box = [0; 4];
        let mut clear = [0.0; 4];
        unsafe {
            gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut scissor_box);
            gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut clear);
            Self {
                read: gl.get_parameter_framebuffer(glow::READ_FRAMEBUFFER_BINDING),
                draw: gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING),
                pack: gl.get_parameter_buffer(glow::PIXEL_PACK_BUFFER_BINDING),
                renderbuffer: gl.get_parameter_renderbuffer(glow::RENDERBUFFER_BINDING),
                alignment: gl.get_parameter_i32(glow::PACK_ALIGNMENT),
                row_length: gl.get_parameter_i32(glow::PACK_ROW_LENGTH),
                skip_rows: gl.get_parameter_i32(glow::PACK_SKIP_ROWS),
                skip_pixels: gl.get_parameter_i32(glow::PACK_SKIP_PIXELS),
                scissor: gl.is_enabled(glow::SCISSOR_TEST),
                scissor_box,
                srgb: gl.is_enabled(glow::FRAMEBUFFER_SRGB),
                clear,
            }
        }
    }

    pub(super) unsafe fn restore(self, gl: &glow::Context) {
        unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, self.read);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, self.draw);
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, self.pack);
            gl.bind_renderbuffer(glow::RENDERBUFFER, self.renderbuffer);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, self.alignment);
            gl.pixel_store_i32(glow::PACK_ROW_LENGTH, self.row_length);
            gl.pixel_store_i32(glow::PACK_SKIP_ROWS, self.skip_rows);
            gl.pixel_store_i32(glow::PACK_SKIP_PIXELS, self.skip_pixels);
            let [x, y, w, h] = self.scissor_box;
            gl.scissor(x, y, w, h);
            enable(gl, glow::SCISSOR_TEST, self.scissor);
            enable(gl, glow::FRAMEBUFFER_SRGB, self.srgb);
            let [r, g, b, a] = self.clear;
            gl.clear_color(r, g, b, a);
        }
    }
}

unsafe fn enable(gl: &glow::Context, capability: u32, enabled: bool) {
    unsafe {
        if enabled {
            gl.enable(capability);
        } else {
            gl.disable(capability);
        }
    }
}

pub(super) struct Targets {
    pub(super) size: Size,
    resolve: glow::Framebuffer,
    resolve_pixels: glow::Renderbuffer,
    output: glow::Framebuffer,
    output_pixels: glow::Renderbuffer,
}

impl Targets {
    pub(super) unsafe fn new(gl: &glow::Context, size: Size) -> Result<Self, &'static str> {
        let limit = unsafe { gl.get_parameter_i32(glow::MAX_RENDERBUFFER_SIZE) }.max(0) as u32;
        if size
            .source
            .into_iter()
            .chain(size.output)
            .any(|edge| edge > limit)
        {
            return Err("This portal exceeds the graphics driver's capture size limit");
        }
        let (resolve, resolve_pixels) = unsafe { surface(gl, size.source) }?;
        let (output, output_pixels) = match unsafe { surface(gl, size.output) } {
            Ok(surface) => surface,
            Err(error) => {
                unsafe {
                    gl.delete_framebuffer(resolve);
                    gl.delete_renderbuffer(resolve_pixels);
                }
                return Err(error);
            }
        };
        Ok(Self {
            size,
            resolve,
            resolve_pixels,
            output,
            output_pixels,
        })
    }

    pub(super) fn byte_len(&self) -> usize {
        self.size.output[0] as usize * self.size.output[1] as usize * 4
    }

    pub(super) unsafe fn capture(
        &self,
        gl: &glow::Context,
        source: Option<glow::Framebuffer>,
        plan: &Plan,
        buffer: glow::Buffer,
    ) {
        let [x, y, w, h] = plan.source;
        let [left, bottom, right, top] = plan.destination;
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            // The actual egui output already contains display-encoded colors.
            // A second sRGB conversion during the blit would brighten them.
            gl.disable(glow::FRAMEBUFFER_SRGB);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, source);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(self.resolve));
            // Slate's default framebuffer is 4x MSAA. Resolve at the exact
            // cropped pixel dimensions: scaled multisample blits are illegal.
            gl.blit_framebuffer(
                x,
                y,
                x + w,
                y + h,
                0,
                0,
                w,
                h,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(self.resolve));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(self.output));
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            // Contain, never stretch. Reversing the destination Y coordinates
            // makes GL's bottom-first read produce top-down encoded rows.
            gl.blit_framebuffer(
                0,
                0,
                w,
                h,
                left,
                top,
                right,
                bottom,
                glow::COLOR_BUFFER_BIT,
                glow::LINEAR,
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(self.output));
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(buffer));
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            gl.pixel_store_i32(glow::PACK_ROW_LENGTH, 0);
            gl.pixel_store_i32(glow::PACK_SKIP_ROWS, 0);
            gl.pixel_store_i32(glow::PACK_SKIP_PIXELS, 0);
            gl.read_pixels(
                0,
                0,
                self.size.output[0] as i32,
                self.size.output[1] as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::BufferOffset(0),
            );
        }
    }

    pub(super) unsafe fn destroy(self, gl: &glow::Context) {
        unsafe {
            gl.delete_framebuffer(self.resolve);
            gl.delete_framebuffer(self.output);
            gl.delete_renderbuffer(self.resolve_pixels);
            gl.delete_renderbuffer(self.output_pixels);
        }
    }
}

unsafe fn surface(
    gl: &glow::Context,
    size: [u32; 2],
) -> Result<(glow::Framebuffer, glow::Renderbuffer), &'static str> {
    const ERROR: &str = "The graphics driver could not create the capture surface";
    let framebuffer = unsafe { gl.create_framebuffer() }.map_err(|_| ERROR)?;
    let pixels = match unsafe { gl.create_renderbuffer() } {
        Ok(pixels) => pixels,
        Err(_) => {
            unsafe { gl.delete_framebuffer(framebuffer) };
            return Err(ERROR);
        }
    };
    unsafe {
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(pixels));
        gl.renderbuffer_storage(
            glow::RENDERBUFFER,
            glow::RGBA8,
            size[0] as i32,
            size[1] as i32,
        );
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(framebuffer));
        gl.framebuffer_renderbuffer(
            glow::DRAW_FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::RENDERBUFFER,
            Some(pixels),
        );
        if gl.check_framebuffer_status(glow::DRAW_FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
            gl.delete_framebuffer(framebuffer);
            gl.delete_renderbuffer(pixels);
            return Err(ERROR);
        }
    }
    Ok((framebuffer, pixels))
}
