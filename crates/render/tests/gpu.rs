//! Test GPU headless (richiedono un adapter wgpu): `cargo test -p render -- --ignored`.
use domain::ElementId;
use egui_wgpu::wgpu;
use geometry::Mesh;
use render::{OrbitCamera, SceneRenderer};

fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))
    .or_else(|_| {
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            force_fallback_adapter: true,
            ..Default::default()
        }))
    })
    .ok()?;
    eprintln!("adapter: {:?}", adapter.get_info());
    pollster::block_on(adapter.request_device(&Default::default())).ok()
}

/// Cubo 1 m (mm) lontano dall'origine mondo.
fn cube(id: u64, at: [f64; 3]) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    let s = 1000.0;
    let faces: [([f32; 3], [[f64; 3]; 4]); 6] = [
        (
            [0., 0., -1.],
            [[0., 0., 0.], [0., s, 0.], [s, s, 0.], [s, 0., 0.]],
        ),
        (
            [0., 0., 1.],
            [[0., 0., s], [s, 0., s], [s, s, s], [0., s, s]],
        ),
        (
            [0., -1., 0.],
            [[0., 0., 0.], [s, 0., 0.], [s, 0., s], [0., 0., s]],
        ),
        (
            [0., 1., 0.],
            [[0., s, 0.], [0., s, s], [s, s, s], [s, s, 0.]],
        ),
        (
            [-1., 0., 0.],
            [[0., 0., 0.], [0., 0., s], [0., s, s], [0., s, 0.]],
        ),
        (
            [1., 0., 0.],
            [[s, 0., 0.], [s, s, 0.], [s, s, s], [s, 0., s]],
        ),
    ];
    for (n, quad) in faces {
        let base = positions.len() as u32;
        for p in quad {
            positions.push([at[0] + p[0], at[1] + p[1], at[2] + p[2]]);
            normals.push(n);
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh {
        positions,
        normals,
        indices,
        element: ElementId::new(id),
    }
}

#[test]
#[ignore = "richiede un adapter GPU"]
fn renders_nonempty_frame() {
    let Some((device, queue)) = device() else {
        panic!("nessun adapter wgpu disponibile");
    };
    let mesh = cube(7, [1.0e7, 1.0e7, 0.0]);
    let camera = OrbitCamera {
        target: [1.0e7 + 500.0, 1.0e7 + 500.0, 500.0],
        distance: 5000.0,
        ..OrbitCamera::default()
    };
    let size = [64, 64];
    let pixels = SceneRenderer::render_offscreen(
        &device,
        &queue,
        std::slice::from_ref(&mesh),
        &camera,
        size,
    );
    let lit = pixels
        .chunks(4)
        .filter(|p| p[0] > 0 || p[1] > 0 || p[2] > 0)
        .count();
    assert!(lit > 0, "frame vuoto");

    let mut r = SceneRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm, None);
    let s = r.upload(&device, &queue, &[mesh]);
    assert_eq!(s.uploaded, 1);
    assert_eq!(
        r.pick(&device, &queue, &camera, size, 32, 32),
        Some(ElementId::new(7))
    );
    assert_eq!(r.pick(&device, &queue, &camera, size, 0, 0), None);

    // upload incrementale: 5 elementi, ne sposto 1 (piccolo drift: niente re-origin)
    let base: Vec<Mesh> = (0..5)
        .map(|i| cube(i, [1.0e7 + 2000.0 * i as f64, 1.0e7, 0.0]))
        .collect();
    let s = r.upload(&device, &queue, &base);
    assert_eq!((s.reused, s.uploaded), (0, 5));
    let origin = r.origin();
    let mut moved = base.clone();
    moved[4] = cube(4, [1.0e7 + 9000.0, 1.0e7 + 500.0, 0.0]);
    let s = r.upload(&device, &queue, &moved);
    assert_eq!((s.reused, s.uploaded, s.removed), (4, 1, 0));
    assert_eq!(r.origin(), origin);
}
