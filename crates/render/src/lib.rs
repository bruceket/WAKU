//! Renderer wgpu di Waku (G0): mesh con shading Lambert (una luce direzionale),
//! colore piatto per elemento, evidenziazione della selezione, camera orbitale
//! relativa a un'origine locale e picking su target offscreen `R32Uint`.

pub mod camera;
pub mod gpu_mesh;
pub mod picking;
pub mod upload;

pub use camera::{OrbitCamera, origin_for, to_local};
pub use picking::{decode_pick_id, encode_pick_id};

use bytemuck::{Pod, Zeroable};
use domain::ElementId;
use egui_wgpu::wgpu;
use geometry::Mesh;
use gpu_mesh::{GpuMesh, InstanceUniform, Vertex};
use std::collections::HashMap;
pub use upload::{UploadPlan, UploadStats, plan_upload};

/// Colore di evidenziazione dell'elemento selezionato (primario ufficiale #BE5532).
pub const SELECTED_COLOR: [f32; 4] = [0.745, 0.333, 0.196, 1.0];

/// Formato depth usato dai pass offscreen (picking e render headless).
const OFFSCREEN_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const PICK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
}

/// Colore piatto deterministico per elemento (palette neutra/blu).
pub fn element_color(id: ElementId) -> [f32; 4] {
    const PALETTE: [[f32; 4]; 4] = [
        [0.80, 0.82, 0.85, 1.0],
        [0.65, 0.72, 0.80, 1.0],
        [0.75, 0.75, 0.70, 1.0],
        [0.60, 0.68, 0.72, 1.0],
    ];
    PALETTE[(id.get() % PALETTE.len() as u64) as usize]
}

/// Renderer di scena. Da inserire in `egui_wgpu::CallbackResources` e usare con [`SceneCallback`].
pub struct SceneRenderer {
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    instance_layout: wgpu::BindGroupLayout,
    mesh_pipeline: wgpu::RenderPipeline,
    pick_pipeline: wgpu::RenderPipeline,
    meshes: Vec<GpuMesh>,
    origin: [f64; 3],
    origin_set: bool,
    selected: Option<ElementId>,
}

impl SceneRenderer {
    /// Crea il renderer per il target colore `format`.
    ///
    /// `depth_format` DEVE coincidere con il formato depth del render pass ospite: in eframe è
    /// quello derivato da `NativeOptions::depth_buffer` (Task 12: `depth_buffer = 32` →
    /// `Depth32Float`). `None` solo se il pass non ha depth attachment (nessun depth test).
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("waku globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_layout = |label| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            })
        };
        let globals_layout = uniform_layout("waku globals layout");
        let instance_layout = uniform_layout("waku instance layout");
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("waku globals bg"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("waku pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&instance_layout)],
            immediate_size: 0,
        });
        let mesh_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("waku mesh.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/mesh.wgsl").into()),
        });
        let pick_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("waku pick.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/pick.wgsl").into()),
        });
        let mesh_pipeline =
            build_pipeline(device, &layout, &mesh_shader, format, depth_format, None);
        let pick_pipeline = build_pipeline(
            device,
            &layout,
            &pick_shader,
            PICK_FORMAT,
            Some(OFFSCREEN_DEPTH_FORMAT),
            None,
        );
        Self {
            globals_buffer,
            globals_bind_group,
            instance_layout,
            mesh_pipeline,
            pick_pipeline,
            meshes: Vec::new(),
            origin: [0.0; 3],
            origin_set: false,
            selected: None,
        }
    }

    /// Origine locale corrente (centro bbox delle mesh caricate).
    pub fn origin(&self) -> [f64; 3] {
        self.origin
    }

    /// Carica le mesh in modo incrementale: riusa i buffer GPU degli elementi con lo stesso
    /// `ElementId` e lo stesso hash di contenuto. L'origine locale si fissa al primo upload non
    /// vuoto e cambia (con ricaricamento completo) solo se il centro bbox si sposta oltre
    /// [`upload::REORIGIN_THRESHOLD_MM`].
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        meshes: &[Mesh],
    ) -> UploadStats {
        let has_geometry = meshes.iter().any(|m| !m.positions.is_empty());
        let center = has_geometry.then(|| origin_for(meshes));
        let (origin, reorigin) =
            upload::next_origin(self.origin_set.then_some(self.origin), center);
        self.origin = origin;
        self.origin_set |= has_geometry;

        let mut current: HashMap<ElementId, GpuMesh> = std::mem::take(&mut self.meshes)
            .into_iter()
            .map(|g| (g.element, g))
            .collect();
        let hashes: HashMap<ElementId, u64> = current.iter().map(|(id, g)| (*id, g.hash)).collect();
        let new: Vec<(ElementId, u64)> = meshes
            .iter()
            .map(|m| (m.element, upload::content_hash(m)))
            .collect();
        let mut stats = upload::plan_upload(&hashes, &new).stats();
        if reorigin {
            // Nuova origine: i vertici locali cambiano, si ricarica tutto.
            current.clear();
            stats.uploaded += stats.reused;
            stats.reused = 0;
        }

        let layout = &self.instance_layout;
        self.meshes = meshes
            .iter()
            .zip(&new)
            .map(|(m, (id, hash))| match current.remove(id) {
                Some(g) if g.hash == *hash => g,
                _ => GpuMesh::new(device, layout, m, origin),
            })
            .collect();
        self.write_instances(queue);
        stats
    }

    /// Imposta l'elemento selezionato (evidenziato con [`SELECTED_COLOR`]).
    pub fn set_selected(&mut self, queue: &wgpu::Queue, selected: Option<ElementId>) {
        self.selected = selected;
        self.write_instances(queue);
    }

    fn write_instances(&self, queue: &wgpu::Queue) {
        for (i, g) in self.meshes.iter().enumerate() {
            let id = g.element;
            let color = if Some(id) == self.selected {
                SELECTED_COLOR
            } else {
                element_color(id)
            };
            let u = InstanceUniform {
                color,
                pick_id: [encode_pick_id(i as u32), 0, 0, 0],
            };
            queue.write_buffer(&g.instance_buffer, 0, bytemuck::bytes_of(&u));
        }
    }

    /// Aggiorna gli uniform di camera (da chiamare prima di `paint`).
    pub fn prepare(&self, queue: &wgpu::Queue, camera: &OrbitCamera, aspect: f32) {
        let vp = camera.view_proj(aspect, self.origin);
        let mut view_proj = [[0.0f32; 4]; 4];
        for (c, col) in view_proj.iter_mut().enumerate() {
            for (r, v) in col.iter_mut().enumerate() {
                *v = vp[(r, c)];
            }
        }
        let g = Globals {
            view_proj,
            light_dir: [-0.4, -0.3, -1.0, 0.0],
        };
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&g));
    }

    /// Disegna le mesh nel pass fornito (camera già impostata con [`SceneRenderer::prepare`]).
    pub fn paint(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.draw(pass, &self.mesh_pipeline);
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>, pipeline: &wgpu::RenderPipeline) {
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.globals_bind_group, &[]);
        for g in &self.meshes {
            pass.set_bind_group(1, &g.instance_bind_group, &[]);
            pass.set_vertex_buffer(0, g.vertex_buffer.slice(..));
            pass.set_index_buffer(g.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..g.index_count, 0, 0..1);
        }
    }

    /// Picking: renderizza gli ID in un target `R32Uint` offscreen di dimensione `size`
    /// e legge il pixel `(x, y)` (pixel fisici, origine in alto a sinistra).
    /// Il readback usa `map_async` e attende la GPU con un poll bloccante.
    pub fn pick(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: &OrbitCamera,
        size: [u32; 2],
        x: u32,
        y: u32,
    ) -> Option<ElementId> {
        if size[0] == 0 || size[1] == 0 || x >= size[0] || y >= size[1] {
            return None;
        }
        self.prepare(queue, camera, size[0] as f32 / size[1] as f32);
        let color = offscreen_texture(device, size, PICK_FORMAT, "waku pick target");
        let depth = offscreen_texture(device, size, OFFSCREEN_DEPTH_FORMAT, "waku pick depth");
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let color_view = color.create_view(&Default::default());
            let depth_view = depth.create_view(&Default::default());
            let mut pass = begin_pass(
                &mut encoder,
                &color_view,
                Some(&depth_view),
                wgpu::Color::TRANSPARENT,
            );
            self.draw(&mut pass, &self.pick_pipeline);
        }
        let value = read_pixels(device, queue, encoder, &color, [x, y], [1, 1], 4);
        let raw = u32::from_le_bytes(value.get(0..4)?.try_into().ok()?);
        let index = decode_pick_id(raw)?;
        self.meshes.get(index as usize).map(|g| g.element)
    }

    /// Render offscreen su una texture RGBA8 (con depth); restituisce i pixel RGBA.
    /// Usato per test headless e screenshot.
    pub fn render_offscreen(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        meshes: &[Mesh],
        camera: &OrbitCamera,
        size: [u32; 2],
    ) -> Vec<u8> {
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let mut r = Self::new(device, format, Some(OFFSCREEN_DEPTH_FORMAT));
        r.upload(device, queue, meshes);
        r.prepare(queue, camera, size[0] as f32 / size[1] as f32);
        let color = offscreen_texture(device, size, format, "waku offscreen");
        let depth = offscreen_texture(device, size, OFFSCREEN_DEPTH_FORMAT, "waku offscreen depth");
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let color_view = color.create_view(&Default::default());
            let depth_view = depth.create_view(&Default::default());
            let mut pass = begin_pass(
                &mut encoder,
                &color_view,
                Some(&depth_view),
                wgpu::Color::BLACK,
            );
            r.paint(&mut pass);
        }
        read_pixels(device, queue, encoder, &color, [0, 0], size, 4)
    }
}

/// Callback egui: aggiorna la camera in `prepare` e disegna in `paint`.
/// Richiede un [`SceneRenderer`] inserito in `CallbackResources`.
pub struct SceneCallback {
    pub camera: OrbitCamera,
    pub aspect: f32,
}

impl egui_wgpu::CallbackTrait for SceneCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(r) = resources.get::<SceneRenderer>() {
            r.prepare(queue, &self.camera, self.aspect);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(r) = resources.get::<SceneRenderer>() {
            r.paint(render_pass);
        }
    }
}

fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("waku pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(Vertex::layout())],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn offscreen_texture(
    device: &wgpu::Device,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    label: &str,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn begin_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    color: &wgpu::TextureView,
    depth: Option<&wgpu::TextureView>,
    clear: wgpu::Color,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("waku offscreen pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: color,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: depth.map(|view| wgpu::RenderPassDepthStencilAttachment {
            view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

/// Copia una regione della texture in un buffer, la mappa (`map_async`) e attende la GPU.
fn read_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    origin: [u32; 2],
    size: [u32; 2],
    bytes_per_pixel: u32,
) -> Vec<u8> {
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let unpadded = size[0] * bytes_per_pixel;
    let padded = unpadded.div_ceil(align) * align;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("waku readback"),
        size: u64::from(padded) * u64::from(size[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: origin[0],
                y: origin[1],
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(size[1]),
            },
        },
        wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let data = slice.get_mapped_range().expect("readback mappato");
    let mut out = Vec::with_capacity((unpadded * size[1]) as usize);
    for row in data.chunks(padded as usize) {
        out.extend_from_slice(&row[..unpadded as usize]);
    }
    drop(data);
    buffer.unmap();
    out
}
