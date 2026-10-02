//! Filtros forenses em Rust puro (só `image` crate): cada submódulo é uma
//! família; todos recebem `&RgbaImage` e devolvem uma nova.

pub mod blur;
pub mod channels;
pub mod compare;
pub mod convolution;
pub mod decorrelation;
pub mod edges;
pub mod enhancement;
pub mod geometric;
pub mod histogram;
pub mod misc;
pub mod morphology;
pub mod tone;
