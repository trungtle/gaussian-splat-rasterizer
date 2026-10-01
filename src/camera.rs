//! Cameras use the COLMAP / 3DGS convention: +x right, +y down, +z forward.
//! That matches Vulkan clip space directly, so no axis flips are needed.

use anyhow::{Context, Result};
use glam::{Mat3, Mat4, Vec3};
use std::path::Path;

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// World -> camera transform.
    pub view: Mat4,
    pub position: Vec3,
    pub fx: f32,
    pub fy: f32,
}

impl Camera {
    pub fn from_c2w(rotation: Mat3, position: Vec3, fx: f32, fy: f32) -> Self {
        let r_t = rotation.transpose();
        let t = -(r_t * position);
        let view = Mat4::from_cols(
            r_t.x_axis.extend(0.0),
            r_t.y_axis.extend(0.0),
            r_t.z_axis.extend(0.0),
            t.extend(1.0),
        );
        Self { view, position, fx, fy }
    }

    /// Camera at `eye` looking at `target`, with `up` as the world up hint.
    /// `fov_y` is the vertical field of view in radians.
    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3, fov_y: f32, height: u32) -> Self {
        let fwd = (target - eye).normalize();
        let right = fwd.cross(up).normalize();
        // +y is down in camera space.
        let down = fwd.cross(right);
        let rot = Mat3::from_cols(right, down, fwd);
        let f = height as f32 * 0.5 / (fov_y * 0.5).tan();
        Self::from_c2w(rot, eye, f, f)
    }

    /// Rescale focal lengths when rendering at a different resolution than the
    /// camera was calibrated for.
    pub fn scaled(mut self, from_width: u32, to_width: u32) -> Self {
        let s = to_width as f32 / from_width as f32;
        self.fx *= s;
        self.fy *= s;
        self
    }

    /// Camera-space +z axis in world space (viewing direction).
    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.view.x_axis.z, self.view.y_axis.z, self.view.z_axis.z)
    }
}

#[derive(serde::Deserialize)]
struct JsonCamera {
    width: u32,
    height: u32,
    position: [f32; 3],
    rotation: [[f32; 3]; 3],
    fx: f32,
    fy: f32,
}

/// A camera from a 3DGS `cameras.json`, with its native resolution.
pub struct DatasetCamera {
    pub camera: Camera,
    pub width: u32,
    pub height: u32,
}

pub fn load_cameras(path: &Path) -> Result<Vec<DatasetCamera>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let cams: Vec<JsonCamera> = serde_json::from_str(&text)?;
    Ok(cams
        .into_iter()
        .map(|c| {
            // `rotation` is the camera-to-world matrix given as rows.
            let r = c.rotation;
            let rot = Mat3::from_cols(
                Vec3::new(r[0][0], r[1][0], r[2][0]),
                Vec3::new(r[0][1], r[1][1], r[2][1]),
                Vec3::new(r[0][2], r[1][2], r[2][2]),
            );
            DatasetCamera {
                camera: Camera::from_c2w(rot, Vec3::from(c.position), c.fx, c.fy),
                width: c.width,
                height: c.height,
            }
        })
        .collect())
}

/// Orbit controller for the interactive viewer.
pub struct Orbit {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub up: Vec3,
    pub fov_y: f32,
}

impl Orbit {
    pub fn camera(&self, height: u32) -> Camera {
        // Build an orthonormal basis around `up` so orbiting works for any up axis.
        let up = self.up.normalize();
        let a = if up.x.abs() < 0.9 { Vec3::X } else { Vec3::Z };
        let e1 = up.cross(a).normalize();
        let e2 = up.cross(e1);
        let dir = (e1 * self.yaw.cos() + e2 * self.yaw.sin()) * self.pitch.cos() + up * self.pitch.sin();
        let eye = self.target + dir * self.distance;
        Camera::look_at(eye, self.target, up, self.fov_y, height)
    }

    /// Initialize the orbit to look from a dataset camera's position.
    pub fn from_camera(cam: &Camera, target: Vec3, up: Vec3, fov_y: f32) -> Self {
        let up = up.normalize();
        let a = if up.x.abs() < 0.9 { Vec3::X } else { Vec3::Z };
        let e1 = up.cross(a).normalize();
        let e2 = up.cross(e1);
        let off = cam.position - target;
        let distance = off.length().max(1e-3);
        let d = off / distance;
        let pitch = d.dot(up).clamp(-1.0, 1.0).asin();
        let yaw = d.dot(e2).atan2(d.dot(e1));
        Self { target, distance, yaw, pitch, up, fov_y }
    }
}
