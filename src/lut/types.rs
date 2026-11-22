#[derive(Clone, Debug, PartialEq)]
pub struct Cube {
    /// Edge length N; `data` holds N^3 entries, red changing fastest.
    pub size: u32,
    pub domain_min: [f32; 3],
    pub domain_max: [f32; 3],
    pub data: Vec<[f32; 3]>,
}
