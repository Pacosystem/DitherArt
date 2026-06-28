use eframe::wgpu;
use eframe::wgpu::util::DeviceExt;
use image::DynamicImage;

use crate::effects::color_adjust::ColorAdjustParams;
use crate::effects::chromatic::ChromaticParams;

// ── WGSL: Color Adjust ──────────────────────────────────────────────────────

const COLOR_ADJUST_WGSL: &str = r#"
struct Params {
    brightness : f32,
    contrast   : f32,
    saturation : f32,
    hue_shift  : f32,
    temperature: f32,
    flags      : u32,   // bit0=grayscale, bit1=sepia, bit2=invert
    width      : u32,
    height     : u32,
}

@group(0) @binding(0) var<storage, read>       inp    : array<u32>;
@group(0) @binding(1) var<storage, read_write> outp   : array<u32>;
@group(0) @binding(2) var<uniform>             params : Params;

fn unpack(v: u32) -> vec4<f32> {
    return vec4<f32>(
        f32((v      ) & 0xFFu) / 255.0,
        f32((v >>  8u) & 0xFFu) / 255.0,
        f32((v >> 16u) & 0xFFu) / 255.0,
        f32((v >> 24u) & 0xFFu) / 255.0,
    );
}

fn pack(c: vec4<f32>) -> u32 {
    let r = u32(clamp(c.r, 0.0, 1.0) * 255.0);
    let g = u32(clamp(c.g, 0.0, 1.0) * 255.0);
    let b = u32(clamp(c.b, 0.0, 1.0) * 255.0);
    let a = u32(clamp(c.a, 0.0, 1.0) * 255.0);
    return r | (g << 8u) | (b << 16u) | (a << 24u);
}

fn rgb_to_hsl(c: vec3<f32>) -> vec3<f32> {
    let mx = max(c.r, max(c.g, c.b));
    let mn = min(c.r, min(c.g, c.b));
    let l  = (mx + mn) * 0.5;
    if (mx - mn) < 0.00001 { return vec3(0.0, 0.0, l); }
    let d = mx - mn;
    let s = select(d / (mx + mn), d / (2.0 - mx - mn), l > 0.5);
    var h: f32;
    if mx == c.r      { h = (c.g - c.b) / d + select(0.0, 6.0, c.g < c.b); }
    else if mx == c.g { h = (c.b - c.r) / d + 2.0; }
    else               { h = (c.r - c.g) / d + 4.0; }
    return vec3(h / 6.0, s, l);
}

fn hue2rgb(p: f32, q: f32, t_in: f32) -> f32 {
    var t = t_in;
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    if t < 1.0/6.0  { return p + (q-p)*6.0*t; }
    if t < 0.5      { return q; }
    if t < 2.0/3.0  { return p + (q-p)*(2.0/3.0 - t)*6.0; }
    return p;
}

fn hsl_to_rgb(hsl: vec3<f32>) -> vec3<f32> {
    if hsl.y < 0.00001 { return vec3(hsl.z); }
    let q = select(hsl.z + hsl.y - hsl.z*hsl.y, hsl.z*(1.0+hsl.y), hsl.z < 0.5);
    let p = 2.0*hsl.z - q;
    return vec3(hue2rgb(p,q,hsl.x+1.0/3.0), hue2rgb(p,q,hsl.x), hue2rgb(p,q,hsl.x-1.0/3.0));
}

fn adj_contrast(v: f32, c: f32) -> f32 {
    let f = (259.0*(c*255.0+255.0)) / (255.0*(259.0-c*255.0));
    return clamp(f*(v-0.5)+0.5, 0.0, 1.0);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.width || id.y >= params.height { return; }
    let i = id.y * params.width + id.x;
    let raw = unpack(inp[i]);
    var rgb = raw.rgb;

    let do_gray  = (params.flags & 1u)  != 0u;
    let do_sepia = (params.flags & 2u)  != 0u;
    let do_inv   = (params.flags & 4u)  != 0u;

    if do_gray || do_sepia {
        let lum = dot(rgb, vec3(0.2126, 0.7152, 0.0722));
        rgb = vec3(lum);
    }
    if do_sepia {
        rgb = vec3(
            clamp(dot(rgb, vec3(0.393, 0.769, 0.189)), 0.0, 1.0),
            clamp(dot(rgb, vec3(0.349, 0.686, 0.168)), 0.0, 1.0),
            clamp(dot(rgb, vec3(0.272, 0.534, 0.131)), 0.0, 1.0),
        );
    }

    if abs(params.hue_shift) > 0.001 || abs(params.saturation) > 0.001 {
        var hsl = rgb_to_hsl(rgb);
        hsl.x = fract(hsl.x + params.hue_shift / 360.0);
        hsl.y = clamp(hsl.y + params.saturation, 0.0, 1.0);
        rgb = hsl_to_rgb(hsl);
    }
    if abs(params.temperature) > 0.001 {
        let t = params.temperature * 0.2;
        rgb.r = clamp(rgb.r + t, 0.0, 1.0);
        rgb.b = clamp(rgb.b - t, 0.0, 1.0);
    }
    if abs(params.brightness) > 0.001 {
        rgb = clamp(rgb + params.brightness, vec3(0.0), vec3(1.0));
    }
    if abs(params.contrast) > 0.001 {
        rgb = vec3(adj_contrast(rgb.r,params.contrast), adj_contrast(rgb.g,params.contrast), adj_contrast(rgb.b,params.contrast));
    }
    if do_inv { rgb = vec3(1.0) - rgb; }

    outp[i] = pack(vec4(rgb, raw.a));
}
"#;

// ── WGSL: Chromatic Aberration (Fixed mode) ─────────────────────────────────

const CHROMATIC_WGSL: &str = r#"
struct Params {
    r_dx: i32, r_dy: i32,
    g_dx: i32, g_dy: i32,
    b_dx: i32, b_dy: i32,
    width : u32,
    height: u32,
}

@group(0) @binding(0) var<storage, read>       inp  : array<u32>;
@group(0) @binding(1) var<storage, read_write> outp : array<u32>;
@group(0) @binding(2) var<uniform>             p    : Params;

fn sample(x: i32, y: i32) -> u32 {
    let cx = clamp(x, 0, i32(p.width)  - 1);
    let cy = clamp(y, 0, i32(p.height) - 1);
    return inp[u32(cy) * p.width + u32(cx)];
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= p.width || id.y >= p.height { return; }
    let x = i32(id.x); let y = i32(id.y);
    let r_px = sample(x + p.r_dx, y + p.r_dy);
    let g_px = sample(x + p.g_dx, y + p.g_dy);
    let b_px = sample(x + p.b_dx, y + p.b_dy);
    let orig = inp[id.y * p.width + id.x];
    let r =  r_px        & 0xFFu;
    let g = (g_px >>  8u) & 0xFFu;
    let b = (b_px >> 16u) & 0xFFu;
    let a = (orig >> 24u) & 0xFFu;
    outp[id.y * p.width + id.x] = r | (g << 8u) | (b << 16u) | (a << 24u);
}
"#;

// ── GPU params structs (must be #[repr(C)] and match WGSL layout) ────────────

#[repr(C)]
struct ColorAdjustGpu {
    brightness: f32, contrast: f32, saturation: f32, hue_shift: f32,
    temperature: f32, flags: u32, width: u32, height: u32,
}

#[repr(C)]
struct ChromaticGpu {
    r_dx: i32, r_dy: i32,
    g_dx: i32, g_dy: i32,
    b_dx: i32, b_dy: i32,
    width: u32, height: u32,
}

// ── Core GPU compute helper ──────────────────────────────────────────────────

pub struct GpuCompute {
    device: wgpu::Device,
    queue: wgpu::Queue,
    color_adjust_pipeline: wgpu::ComputePipeline,
    color_adjust_bgl: wgpu::BindGroupLayout,
    chromatic_pipeline: wgpu::ComputePipeline,
    chromatic_bgl: wgpu::BindGroupLayout,
}

fn make_pipeline(
    device: &wgpu::Device,
    wgsl: &str,
    label: &str,
) -> (wgpu::ComputePipeline, wgpu::BindGroupLayout) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    // Derive bind group layout from pipeline (auto-layout)
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: None, // auto-layout
        module: &shader,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let bgl = pipeline.get_bind_group_layout(0);
    (pipeline, bgl)
}

impl GpuCompute {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let (color_adjust_pipeline, color_adjust_bgl) =
            make_pipeline(&device, COLOR_ADJUST_WGSL, "color_adjust");
        let (chromatic_pipeline, chromatic_bgl) =
            make_pipeline(&device, CHROMATIC_WGSL, "chromatic");
        Self { device, queue, color_adjust_pipeline, color_adjust_bgl, chromatic_pipeline, chromatic_bgl }
    }

    fn run_compute(
        &self,
        pipeline: &wgpu::ComputePipeline,
        bgl: &wgpu::BindGroupLayout,
        input_bytes: &[u8],
        params_bytes: &[u8],
        output_size: u64,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let device = &self.device;
        let queue = &self.queue;

        let input_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: input_bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });
        let output_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: output_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Pad params to minimum uniform buffer size (256 bytes)
        let mut padded_params = params_bytes.to_vec();
        let align = 256usize;
        if padded_params.len() < align {
            padded_params.resize(align, 0);
        }
        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &padded_params,
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: input_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: output_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: params_buf.as_entire_binding() },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups((width + 15) / 16, (height + 15) / 16, 1);
        }
        encoder.copy_buffer_to_buffer(&output_buf, 0, &staging_buf, 0, output_size);
        queue.submit(std::iter::once(encoder.finish()));

        // Read back synchronously
        let slice = staging_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
        device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().unwrap();
        let data = slice.get_mapped_range();
        let result = data.to_vec();
        drop(data);
        staging_buf.unmap();
        result
    }

    pub fn color_adjust(&self, img: &DynamicImage, p: &ColorAdjustParams) -> DynamicImage {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let raw = rgba.as_raw();
        let output_size = (w * h * 4) as u64;

        let flags: u32 = (p.grayscale as u32) | ((p.sepia as u32) << 1) | ((p.invert as u32) << 2);
        let gpu_params = ColorAdjustGpu {
            brightness: p.brightness, contrast: p.contrast,
            saturation: p.saturation, hue_shift: p.hue_shift,
            temperature: p.temperature, flags, width: w, height: h,
        };
        let params_bytes = unsafe {
            std::slice::from_raw_parts(
                &gpu_params as *const ColorAdjustGpu as *const u8,
                std::mem::size_of::<ColorAdjustGpu>(),
            )
        };

        let out = self.run_compute(
            &self.color_adjust_pipeline, &self.color_adjust_bgl,
            raw, params_bytes, output_size, w, h,
        );
        let out_buf = image::ImageBuffer::from_raw(w, h, out).unwrap();
        DynamicImage::ImageRgba8(out_buf)
    }

    pub fn chromatic_fixed(&self, img: &DynamicImage, p: &ChromaticParams) -> DynamicImage {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let raw = rgba.as_raw();
        let output_size = (w * h * 4) as u64;

        let gpu_params = ChromaticGpu {
            r_dx: p.r_dx, r_dy: p.r_dy,
            g_dx: p.g_dx, g_dy: p.g_dy,
            b_dx: p.b_dx, b_dy: p.b_dy,
            width: w, height: h,
        };
        let params_bytes = unsafe {
            std::slice::from_raw_parts(
                &gpu_params as *const ChromaticGpu as *const u8,
                std::mem::size_of::<ChromaticGpu>(),
            )
        };

        let out = self.run_compute(
            &self.chromatic_pipeline, &self.chromatic_bgl,
            raw, params_bytes, output_size, w, h,
        );
        let out_buf = image::ImageBuffer::from_raw(w, h, out).unwrap();
        DynamicImage::ImageRgba8(out_buf)
    }
}
