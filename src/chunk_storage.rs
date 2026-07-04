use crate::chunk::{self, CHUNK_SIZE};
use crate::lattice;

pub const CHUNK_XZ: usize = lattice::XZ as usize / CHUNK_SIZE;
pub const CHUNK_Y: usize = lattice::Y as usize / CHUNK_SIZE;
pub const CHUNK_COUNT: usize = CHUNK_XZ * CHUNK_XZ * CHUNK_Y;

const CHUNK_BUFFER_SIZE: usize = (lattice::XZ * lattice::XZ * lattice::Y) as usize;

type ChunkRenderBuffer = [u32; CHUNK_BUFFER_SIZE];

pub struct ChunkStorage {
    chunks: [chunk::Chunk; CHUNK_COUNT],
}

impl ChunkStorage {
    pub fn new() -> Self {
        let mut chunks = [chunk::Chunk::default(); CHUNK_COUNT];
        for x in 0..CHUNK_XZ {
            for z in 0..CHUNK_XZ {
                for y in 0..CHUNK_Y {
                    chunks[z + x * CHUNK_XZ + y * CHUNK_XZ * CHUNK_XZ] =
                        chunk::Chunk::noise(x as i32, y as i32, z as i32);
                    //     chunk::Chunk::empty();
                    // if y == 0 {
                    //     for cx in 0..CHUNK_SIZE {
                    //         for cz in 0..CHUNK_SIZE {
                    //             chunks[z + x * CHUNK_XZ + y * CHUNK_XZ * CHUNK_XZ].blocks[cx][cz] =
                    //                 1;
                    //         }
                    //     }
                    //     chunks[z + x * CHUNK_XZ + y * CHUNK_XZ * CHUNK_XZ].blocks[0][0] = u32::MAX;
                    // }
                }
            }
        }

        Self { chunks }
    }

    pub fn get(&self, x: isize, y: isize, z: isize) -> Option<&chunk::Chunk> {
        if x < 0 || y < 0 || z < 0 {
            return None;
        }
        let (x, y, z) = (x as usize, y as usize, z as usize);
        self.chunks.get(x * CHUNK_XZ + z + y * CHUNK_XZ * CHUNK_XZ)
    }

    pub fn copy_to_render_buffer(&self, x: isize, y: isize, z: isize) -> ChunkRenderBuffer {
        let mut buf: ChunkRenderBuffer = [0; _];

        for chunk_z in 0..CHUNK_XZ as usize {
            for chunk_x in 0..CHUNK_XZ as usize {
                for chunk_y in 0..CHUNK_Y as usize {
                    let Some(chunk) = self.get(
                        chunk_x as isize + x,
                        chunk_y as isize + y,
                        chunk_z as isize + z,
                    ) else {
                        continue;
                    };
                    let base_x = chunk_x * CHUNK_SIZE;
                    let base_y = chunk_y * CHUNK_SIZE;
                    let base_z = chunk_z * CHUNK_SIZE;

                    for z in 0..CHUNK_SIZE {
                        for x in 0..CHUNK_SIZE {
                            for y in 0..CHUNK_SIZE {
                                let gx = base_x + x;
                                let gy = base_y + y;
                                let gz = base_z + z;

                                buf[gx
                                    + gz * lattice::XZ as usize
                                    + gy * (lattice::XZ * lattice::XZ) as usize] =
                                    (chunk.blocks[x][z] >> y) & 1;

                                // (gx as u32) << 16 | (gy as u32) << 8 | (gz as u32);
                            }
                        }
                    }
                }
            }
        }
        buf
    }
}
