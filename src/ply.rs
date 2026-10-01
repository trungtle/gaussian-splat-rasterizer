//! Loader for the 3D Gaussian Splatting PLY format (Kerbl et al. 2023).
//!
//! Supports `binary_little_endian` files with float properties. Properties are
//! located by name, so files with or without normals / higher-order SH load.

use anyhow::{Context, Result, bail};
use glam::{Mat3, Quat, Vec3};
use std::path::Path;

/// Number of SH coefficients stored per channel (degree 3 => 16).
pub const SH_COEFFS: usize = 16;

/// GPU-ready per-splat data (std430 compatible, 48 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Splat {
    /// xyz = world position, w = opacity (post-sigmoid).
    pub pos_opacity: [f32; 4],
    /// Upper triangle of the 3D covariance: xx, xy, xz, yy.
    pub cov_a: [f32; 4],
    /// yz, zz, unused, unused.
    pub cov_b: [f32; 4],
}

pub struct Scene {
    pub splats: Vec<Splat>,
    /// `SH_COEFFS * 3` floats per splat, laid out `[coeff][rgb]`.
    pub sh: Vec<f32>,
    /// SH degree present in the file (0..=3).
    pub sh_degree: u32,
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

pub fn load(path: &Path) -> Result<Scene> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let marker = b"end_header\n";
    let header_end = bytes
        .windows(marker.len())
        .position(|w| w == marker)
        .context("PLY: missing end_header")?
        + marker.len();
    let header = std::str::from_utf8(&bytes[..header_end]).context("PLY: header not utf8")?;

    let mut count = 0usize;
    let mut props: Vec<String> = Vec::new();
    let mut in_vertex = false;
    for line in header.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        match t.as_slice() {
            ["format", fmt, ..] if *fmt != "binary_little_endian" => {
                bail!("PLY: unsupported format {fmt}")
            }
            ["element", name, n] => {
                in_vertex = *name == "vertex";
                if in_vertex {
                    count = n.parse()?;
                }
            }
            ["property", ty, name] if in_vertex => {
                if *ty != "float" && *ty != "float32" {
                    bail!("PLY: unsupported property type {ty} for {name}");
                }
                props.push(name.to_string());
            }
            _ => {}
        }
    }

    let stride = props.len();
    let idx = |name: &str| props.iter().position(|p| p == name);
    let need = |name: &str| idx(name).with_context(|| format!("PLY: missing property {name}"));
    let [x, y, z] = [need("x")?, need("y")?, need("z")?];
    let dc = [need("f_dc_0")?, need("f_dc_1")?, need("f_dc_2")?];
    let opacity = need("opacity")?;
    let scale = [need("scale_0")?, need("scale_1")?, need("scale_2")?];
    let rot = [need("rot_0")?, need("rot_1")?, need("rot_2")?, need("rot_3")?];
    let rest: Vec<usize> = (0..).map_while(|i| idx(&format!("f_rest_{i}"))).collect();
    let rest_per_channel = rest.len() / 3;
    let sh_degree = match rest_per_channel {
        0 => 0,
        3 => 1,
        8 => 2,
        15 => 3,
        n => bail!("PLY: unexpected f_rest count {}", n * 3),
    };

    let body = &bytes[header_end..];
    if body.len() < count * stride * 4 {
        bail!("PLY: truncated body");
    }
    // The body starts at an arbitrary byte offset, so decode into an aligned buffer.
    let floats: Vec<f32> = body[..count * stride * 4]
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();

    let mut splats = Vec::with_capacity(count);
    let mut sh = vec![0.0f32; count * SH_COEFFS * 3];

    for i in 0..count {
        let v = &floats[i * stride..(i + 1) * stride];
        let pos = Vec3::new(v[x], v[y], v[z]);

        let s = Vec3::new(v[scale[0]].exp(), v[scale[1]].exp(), v[scale[2]].exp());
        // PLY stores the quaternion as (w, x, y, z).
        let q = Quat::from_xyzw(v[rot[1]], v[rot[2]], v[rot[3]], v[rot[0]]).normalize();
        let m = Mat3::from_quat(q) * Mat3::from_diagonal(s);
        let c = m * m.transpose();

        splats.push(Splat {
            pos_opacity: [pos.x, pos.y, pos.z, sigmoid(v[opacity])],
            cov_a: [c.x_axis.x, c.y_axis.x, c.z_axis.x, c.y_axis.y],
            cov_b: [c.z_axis.y, c.z_axis.z, 0.0, 0.0],
        });

        let out = &mut sh[i * SH_COEFFS * 3..(i + 1) * SH_COEFFS * 3];
        for ch in 0..3 {
            out[ch] = v[dc[ch]];
            // f_rest is channel-major: [R coeffs 1..n][G ...][B ...].
            for k in 0..rest_per_channel {
                out[(k + 1) * 3 + ch] = v[rest[ch * rest_per_channel + k]];
            }
        }
    }

    Ok(Scene { splats, sh, sh_degree })
}
