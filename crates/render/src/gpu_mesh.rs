//! Mesh caricate su GPU (vertici in coordinate locali f32).
use crate::camera::to_local;
use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu::{self, util::DeviceExt};
use geometry::Mesh;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

impl Vertex {
    pub const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Uniform per-mesh: colore e ID di picking (vec4 per allineamento std140).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct InstanceUniform {
    pub color: [f32; 4],
    pub pick_id: [u32; 4],
}

/// Vertici locali (f64 → f32 relativi a `origin`).
pub fn local_vertices(mesh: &Mesh, origin: [f64; 3]) -> Vec<Vertex> {
    mesh.positions
        .iter()
        .zip(mesh.normals.iter())
        .map(|(p, n)| Vertex {
            position: to_local(*p, origin),
            normal: *n,
        })
        .collect()
}

pub struct GpuMesh {
    pub element: domain::ElementId,
    pub hash: u64,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub instance_buffer: wgpu::Buffer,
    pub instance_bind_group: wgpu::BindGroup,
}

impl GpuMesh {
    pub fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        mesh: &Mesh,
        origin: [f64; 3],
    ) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("waku mesh vertices"),
            contents: bytemuck::cast_slice(&local_vertices(mesh, origin)),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("waku mesh indices"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("waku mesh instance"),
            size: std::mem::size_of::<InstanceUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instance_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("waku mesh instance bg"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: instance_buffer.as_entire_binding(),
            }],
        });
        Self {
            element: mesh.element,
            hash: crate::upload::content_hash(mesh),
            vertex_buffer,
            index_buffer,
            index_count: mesh.indices.len() as u32,
            instance_buffer,
            instance_bind_group,
        }
    }
}
