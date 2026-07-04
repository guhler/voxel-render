#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlobalsUniform {
    pub proj_view_mat: [[f32; 4]; 4],
    pub light_mat: [[f32; 4]; 4],
    pub cam_pos: [f32; 3],
    pub _pad0: u32,
    pub cam_dir: [f32; 3],
    pub _pad1: u32,
    pub light_pos: [f32; 3],
    pub _pad2: u32,
    pub light_dir: [f32; 3],
    pub _pad3: u32,
    pub grid_lines: u32,
    pub _pad4: [u32; 3],
}

impl Default for GlobalsUniform {
    fn default() -> Self {
        Self {
            proj_view_mat: [[0.0; 4]; 4],
            light_mat: [[0.0; 4]; 4],
            cam_pos: [0.0; 3],
            cam_dir: [0.0; 3],
            light_pos: [0.0; 3],
            light_dir: [0.0; 3],
            grid_lines: 0,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
            _pad3: 0,
            _pad4: [0; 3],
        }
    }
}
