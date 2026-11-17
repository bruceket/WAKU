//! Camera orbitale con coordinate relative a un'origine locale (f64 → f32).
use geometry::Mesh;
use nalgebra::{Matrix4, Point3, Vector3};

/// Converte una posizione mondo f64 in coordinate locali f32 sottraendo `origin`.
/// La sottrazione avviene in f64: la precisione f32 si applica solo all'offset.
pub fn to_local(p: [f64; 3], origin: [f64; 3]) -> [f32; 3] {
    [
        (p[0] - origin[0]) as f32,
        (p[1] - origin[1]) as f32,
        (p[2] - origin[2]) as f32,
    ]
}

/// Centro del bounding box di tutte le mesh (origine vuota → `[0,0,0]`).
pub fn origin_for(meshes: &[Mesh]) -> [f64; 3] {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for p in meshes.iter().flat_map(|m| m.positions.iter()) {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    if min[0] > max[0] {
        return [0.0; 3];
    }
    [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ]
}

/// Limite del pitch (evita la degenerazione di look_at ai poli).
pub const MAX_PITCH: f64 = std::f64::consts::FRAC_PI_2 - 0.01;

/// Camera orbitale (Z verso l'alto). Angoli in radianti, distanza in unità modello (mm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitCamera {
    pub target: [f64; 3],
    pub yaw: f64,
    pub pitch: f64,
    pub distance: f64,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: [0.0; 3],
            yaw: std::f64::consts::FRAC_PI_4,
            pitch: 0.6,
            distance: 20_000.0,
        }
    }
}

impl OrbitCamera {
    /// Posizione dell'occhio in coordinate mondo.
    pub fn eye(&self) -> [f64; 3] {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.clamp(-MAX_PITCH, MAX_PITCH).sin_cos();
        [
            self.target[0] + self.distance * cp * cy,
            self.target[1] + self.distance * cp * sy,
            self.target[2] + self.distance * sp,
        ]
    }

    /// Matrice view-projection (convenzione wgpu, depth 0..1) relativa a `origin`.
    pub fn view_proj(&self, aspect: f32, origin: [f64; 3]) -> Matrix4<f32> {
        let eye = to_local(self.eye(), origin);
        let target = to_local(self.target, origin);
        let view = Matrix4::look_at_rh(&Point3::from(eye), &Point3::from(target), &Vector3::z());
        let near = (self.distance as f32 * 0.001).max(1.0);
        let far = self.distance as f32 * 100.0 + 1.0e5;
        let fovy = 45f32.to_radians();
        // Prospettiva RH con depth [0,1] (wgpu).
        let f = 1.0 / (fovy * 0.5).tan();
        let aspect = if aspect > 0.0 { aspect } else { 1.0 };
        #[rustfmt::skip]
        let proj = Matrix4::new(
            f / aspect, 0.0, 0.0, 0.0,
            0.0, f, 0.0, 0.0,
            0.0, 0.0, far / (near - far), near * far / (near - far),
            0.0, 0.0, -1.0, 0.0,
        );
        proj * view
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::ElementId;

    #[test]
    fn camera_relative_precision() {
        let origin = [1.0e7, 1.0e7, 0.0];
        let a = to_local([1.0e7, 1.0e7, 0.0], origin);
        let b = to_local([1.0e7 + 1.0, 1.0e7, 0.0], origin);
        assert_ne!(a, b);
        assert!((b[0] - a[0] - 1.0).abs() < 1e-3);
        // senza origine locale i due punti collasserebbero in f32? verifica che l'offset sia piccolo
        assert!(b[0].abs() < 2.0);
    }

    #[test]
    fn origin_centered() {
        let m = Mesh {
            positions: vec![[1.0e7, 0.0, 0.0], [1.0e7 + 2000.0, 4000.0, 3000.0]],
            normals: vec![[0.0, 0.0, 1.0]; 2],
            indices: vec![],
            element: ElementId::new(1),
        };
        assert_eq!(origin_for(&[m]), [1.0e7 + 1000.0, 2000.0, 1500.0]);
        assert_eq!(origin_for(&[]), [0.0; 3]);
    }

    #[test]
    fn view_proj_target_projects_to_center() {
        let cam = OrbitCamera {
            target: [1.0e7, 1.0e7, 0.0],
            ..OrbitCamera::default()
        };
        let vp = cam.view_proj(1.5, cam.target);
        let c = vp * nalgebra::Vector4::new(0.0, 0.0, 0.0, 1.0);
        assert!((c.x / c.w).abs() < 1e-4 && (c.y / c.w).abs() < 1e-4);
        let z = c.z / c.w;
        assert!((0.0..=1.0).contains(&z));
    }
}

#[cfg(test)]
mod pitch_tests {
    use super::*;

    #[test]
    fn pitch_clamped_at_pole() {
        let cam = OrbitCamera {
            pitch: 10.0,
            distance: 1.0,
            ..OrbitCamera::default()
        };
        let e = cam.eye();
        assert!(e[0].hypot(e[1]) > 1e-3, "l'occhio non deve stare sul polo");
        assert!(cam.view_proj(1.0, [0.0; 3]).iter().all(|v| v.is_finite()));
    }
}
