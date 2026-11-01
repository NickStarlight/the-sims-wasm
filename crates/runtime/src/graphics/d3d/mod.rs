//! Owned Direct3D COM state, adapted from the original working software renderer.
mod clear;
mod device;
mod draw;
mod enumerate;
pub(super) mod interfaces;
mod math;
mod objects;
mod query;
mod raster;
mod resources;
mod state;
mod texture;
mod texture_objects;
mod vertices;
mod viewport;
use super::*;
use resources::{Light, Material};
use viewport::Viewport;
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Interface {
    Parent1,
    Parent3,
    Device2,
    Device3,
    Viewport,
    Material1,
    Material3,
    Light,
    Texture1,
    Texture2,
}
#[derive(Default)]
pub(super) struct State {
    parents: BTreeMap<u32, u32>,
    devices: BTreeMap<u32, device::Device>,
    device_aliases: BTreeMap<u32, u32>,
    viewports: BTreeMap<u32, Viewport>,
    materials: BTreeMap<u32, Material>,
    lights: BTreeMap<u32, Light>,
    textures: BTreeMap<u32, u32>,
    next_material: u32,
}

#[cfg(test)]
#[path = "../../../tests/unit/graphics/d3d/mod.rs"]
mod tests;
