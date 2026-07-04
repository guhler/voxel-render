use std::{num::NonZero, sync::Arc, time};

use cgmath::{InnerSpace, Zero};
use wgpu::{util::DeviceExt, TextureView};

use crate::{
    background,
    camera::{Camera, CameraController},
    chunk, chunk_storage,
    globals::{self, GlobalsUniform},
    lattice, shadow,
    vertex::Vertex,
};

const SHADOW_MAP_RES: u32 = 8 * 1024;

pub struct State<'a> {
    instance: wgpu::Instance,
    surface: wgpu::Surface<'a>,
    surface_config: wgpu::SurfaceConfiguration,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    background_pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    window: Arc<winit::window::Window>,

    bind_group: wgpu::BindGroup,
    background_bind_group: wgpu::BindGroup,
    shadow_bind_group: wgpu::BindGroup,

    camera: Camera,
    camera_controller: CameraController,
    light_dir: cgmath::Vector3<f32>,
    grid_lines: bool,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    depth_texture: wgpu::Texture,
    multisample_texture: wgpu::Texture,
    background_vertices: wgpu::Buffer,
    globals_buf: wgpu::Buffer,
    chunk_texture: wgpu::Texture,
    background_buf: wgpu::Buffer,
    shadow_map: wgpu::Texture,

    chunks: chunk_storage::ChunkStorage,

    frame_count: u32,
    start: time::Instant,
    last_render: time::Instant,
    last_print: time::Instant,
    start_instant: time::Instant,
}

impl<'a> State<'a> {
    const INIT_WIDTH: u32 = 800;
    const INIT_HEIGHT: u32 = 600;

    pub async fn new(window: winit::window::Window) -> Self {
        let window = Arc::new(window);

        let instance = wgpu::Instance::default();

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .unwrap();

        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();

        let surface_config = Self::configure_surface(&surface, &adapter, &device);

        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("view matrix buffer"),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            size: std::mem::size_of::<globals::GlobalsUniform>() as u64,
            mapped_at_creation: false,
        });

        let chunks = chunk_storage::ChunkStorage::new();

        let chunk_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("chunk texture"),
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            size: wgpu::Extent3d {
                width: lattice::XZ,
                height: lattice::XZ,
                depth_or_array_layers: lattice::Y,
            },
            mip_level_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::R32Uint,
            sample_count: 1,
            view_formats: &[],
        });

        let shadow_map = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow map texture"),
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            size: wgpu::Extent3d {
                width: SHADOW_MAP_RES,
                height: SHADOW_MAP_RES,
                depth_or_array_layers: 1,
            },
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            mip_level_count: 1,
            sample_count: 1,
            view_formats: &[],
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D3,
                    },
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("view matrix uniform"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &chunk_texture.create_view(&Default::default()),
                    ),
                },
            ],
        });

        let shadow_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("shadow bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        count: None,
                    },
                ],
            });

        let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow bind group"),
            layout: &shadow_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&shadow_map.create_view(
                        &wgpu::TextureViewDescriptor {
                            usage: Some(
                                wgpu::TextureUsages::RENDER_ATTACHMENT
                                    | wgpu::TextureUsages::TEXTURE_BINDING,
                            ),
                            ..Default::default()
                        },
                    )),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&device.create_sampler(
                        &wgpu::SamplerDescriptor {
                            label: Some("shadow map sampler"),
                            address_mode_u: wgpu::AddressMode::ClampToEdge,
                            address_mode_v: wgpu::AddressMode::ClampToEdge,
                            address_mode_w: wgpu::AddressMode::ClampToEdge,
                            mag_filter: wgpu::FilterMode::Linear,
                            min_filter: wgpu::FilterMode::Linear,
                            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                            lod_min_clamp: 0.0,
                            lod_max_clamp: 0.0,
                            compare: Some(wgpu::CompareFunction::Less),
                            anisotropy_clamp: 1,
                            border_color: None,
                        },
                    )),
                },
            ],
        });

        let shader_source =
            std::fs::read_to_string("./src/background.wgsl").expect("missing background.wgsl");
        let background_shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("background shader module"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source.as_str())),
        });

        let shader_source =
            std::fs::read_to_string("./src/shader.wgsl").expect("missing shader.wgsl");
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("main shader module"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source.as_str())),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render pipeline"),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_compare: Some(wgpu::CompareFunction::Less),
                depth_write_enabled: Some(true),
                bias: Default::default(),
                stencil: Default::default(),
            }),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("pipeline layout descriptor"),
                    bind_group_layouts: &[
                        Some(&bind_group_layout),
                        Some(&shadow_bind_group_layout),
                    ],
                    ..Default::default()
                }),
            ),
            multisample: wgpu::MultisampleState {
                count: 4,
                mask: 0xffffffff,
                alpha_to_coverage_enabled: false,
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Cw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            vertex: wgpu::VertexState {
                buffers: &[wgpu::VertexBufferLayout {
                    step_mode: Default::default(),
                    attributes: &Vertex::attributes(),
                    array_stride: Vertex::stride(),
                }],
                compilation_options: Default::default(),
                entry_point: Some("vx_main"),
                module: &shader_module,
            },
            fragment: Some(wgpu::FragmentState {
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_config.format,
                    blend: None,
                    write_mask: Default::default(),
                })],
                compilation_options: Default::default(),
                entry_point: Some("fg_main"),
                module: &shader_module,
            }),
            multiview_mask: None,
            cache: None,
        });

        let (background_buf, background_bind_group_layout, background_bind_group) =
            background::background(&device);

        let background_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("background render pipeline"),
            depth_stencil: None,
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("pipeline layout descriptor"),
                    bind_group_layouts: &[Some(&background_bind_group_layout)],
                    ..Default::default()
                }),
            ),
            multisample: Default::default(),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Cw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            vertex: wgpu::VertexState {
                buffers: &[wgpu::VertexBufferLayout {
                    step_mode: Default::default(),
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                    array_stride: std::mem::size_of::<[f32; 3]>() as u64,
                }],
                compilation_options: Default::default(),
                entry_point: Some("vx_main"),
                module: &background_shader_module,
            },
            fragment: Some(wgpu::FragmentState {
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: Default::default(),
                })],
                compilation_options: Default::default(),
                entry_point: Some("fg_main"),
                module: &background_shader_module,
            }),
            multiview_mask: None,
            cache: None,
        });

        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow render pipeline"),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_compare: Some(wgpu::CompareFunction::Less),
                depth_write_enabled: Some(true),
                bias: wgpu::DepthBiasState {
                    clamp: 0.0,
                    constant: 1,
                    slope_scale: 4.0,
                },
                stencil: Default::default(),
            }),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("shadow pipeline layout descriptor"),
                    bind_group_layouts: &[Some(&bind_group_layout)],
                    ..Default::default()
                }),
            ),
            multisample: Default::default(),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Cw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            vertex: wgpu::VertexState {
                buffers: &[wgpu::VertexBufferLayout {
                    step_mode: Default::default(),
                    attributes: &Vertex::attributes(),
                    array_stride: Vertex::stride(),
                }],
                compilation_options: Default::default(),
                entry_point: Some("vx_shadow"),
                module: &shader_module,
            },
            fragment: Some(wgpu::FragmentState {
                targets: &[],
                compilation_options: Default::default(),
                entry_point: Some("fg_shadow"),
                module: &shader_module,
            }),
            multiview_mask: None,
            cache: None,
        });

        let lattice = lattice::Lattice::new();

        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertex buffer"),
            usage: wgpu::BufferUsages::VERTEX,
            contents: bytemuck::cast_slice(&lattice.vertices),
        });

        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("index buffer"),
            usage: wgpu::BufferUsages::INDEX,
            contents: bytemuck::cast_slice(&lattice.indices),
        });

        let depth_texture = Self::create_depth_texture(&device, &surface_config);
        let multisample_texture = Self::create_multisample_texture(&device, &surface_config);

        let background_vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("background vertices"),
            usage: wgpu::BufferUsages::VERTEX,
            contents: bytemuck::cast_slice(&[
                [-1.0f32, -1.0, 0.0],
                [-1.0, 1.0, 0.0],
                [1.0, -1.0, 0.0],
                [-1.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
                [1.0, -1.0, 0.0],
            ]),
        });

        Self {
            camera: Camera::new(Self::INIT_WIDTH as f32 / Self::INIT_HEIGHT as f32),
            camera_controller: CameraController::new(12.0, 0.002),
            grid_lines: false,

            instance,
            surface,
            surface_config,
            adapter,
            device,
            queue,
            pipeline,
            background_pipeline,
            shadow_pipeline,
            window,

            bind_group,
            background_bind_group,
            shadow_bind_group,

            vertex_buf,
            index_buf,
            depth_texture,
            multisample_texture,
            background_vertices,
            globals_buf,
            chunk_texture,
            background_buf,

            light_dir: cgmath::Vector3::new(0.0, -1.0, 0.0),
            shadow_map,

            chunks,

            frame_count: 0,
            start: time::Instant::now(),
            last_render: time::Instant::now(),
            last_print: time::Instant::now(),
            start_instant: time::Instant::now(),
        }
    }

    pub fn recreate_surface(&mut self) {
        self.instance.create_surface(self.window.clone()).unwrap();
        Self::configure_surface(&mut self.surface, &self.adapter, &self.device);
    }

    pub fn configure_surface(
        surface: &wgpu::Surface,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
    ) -> wgpu::SurfaceConfiguration {
        let mut surface_config = surface
            .get_default_config(adapter, Self::INIT_WIDTH, Self::INIT_HEIGHT)
            .unwrap();
        surface_config.present_mode = wgpu::PresentMode::Fifo;

        surface.configure(device, &surface_config);
        surface_config
    }

    pub fn window(&self) -> &winit::window::Window {
        &self.window
    }

    pub fn device_input(&mut self, event: winit::event::DeviceEvent) {
        use winit::event::DeviceEvent;
        match event {
            DeviceEvent::MouseMotion { delta: (dx, dy) } => {
                self.camera_controller.process_mouse(dx as f32, dy as f32)
            }
            _ => (),
        }
    }

    pub fn window_input(&mut self, event: winit::event::WindowEvent) {
        use winit::event::WindowEvent;
        match event {
            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key: winit::keyboard::PhysicalKey::Code(code),
                        state,
                        ..
                    },
                ..
            } => {
                let pressed = state == winit::event::ElementState::Pressed;
                self.camera_controller.process_keyboard(code, pressed);
                if code == winit::keyboard::KeyCode::F3 && pressed {
                    self.grid_lines = !self.grid_lines;
                }
            }
            WindowEvent::MouseWheel {
                delta: winit::event::MouseScrollDelta::LineDelta(_, d),
                ..
            } => {
                self.camera_controller.process_scroll(d);
            }
            _ => (),
        }
    }

    pub fn update(&mut self) {}

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.surface_config.width = new_size.width;
        self.surface_config.height = new_size.height;
        self.surface.configure(&self.device, &self.surface_config);

        self.camera.resize(new_size.width, new_size.height);
        self.multisample_texture =
            Self::create_multisample_texture(&self.device, &self.surface_config);
        self.depth_texture = Self::create_depth_texture(&self.device, &self.surface_config);
    }

    pub fn width(&self) -> u32 {
        self.surface_config.width
    }

    pub fn height(&self) -> u32 {
        self.surface_config.height
    }

    pub fn render(&mut self) -> Option<wgpu::CurrentSurfaceTexture> {
        let now = time::Instant::now();

        let elapsed = now - self.last_print;
        if elapsed > std::time::Duration::from_secs(1) {
            println!("AVG frame time: {:?}", elapsed / self.frame_count);

            self.frame_count = 0;
            self.last_print = now;
        }

        let elapsed = now - self.last_render;
        self.last_render = now;
        self.camera_controller
            .update_camera(&mut self.camera, elapsed);

        self.light_dir.x = f32::sin((now - self.start).as_secs_f32() * 0.2);
        self.light_dir.z = f32::cos((now - self.start).as_secs_f32() * 0.3);

        self.queue.write_buffer(
            &self.background_buf,
            0,
            bytemuck::bytes_of(&background::BackgroundUniform {
                resolution: [self.width(), self.height()],
                millis_elapsed: (now - self.start_instant).as_millis() as u32,
                pitch: self.camera.pitch.0,
                yaw: self.camera.yaw.0,
                fovy: self.camera.fovy.0,
            }),
        );

        let cam_mat = self.camera.proj_view_matrix();
        let dir = self.camera.direction();
        let pos = self.camera.position();
        let light_pos = cgmath::Vector3::zero() - self.light_dir.normalize() * 128.0;
        let light = self.light_dir.normalize();
        let globals = GlobalsUniform {
            proj_view_mat: cam_mat.into(),
            light_mat: shadow::directional(light_pos, light).into(),
            cam_dir: [dir.x, dir.y, dir.z],
            cam_pos: [pos.x, pos.y, pos.z],
            light_pos: [light_pos.x, light_pos.y, light_pos.z],
            light_dir: [light.x, light.y, light.z],
            grid_lines: self.grid_lines as u32,
            ..Default::default()
        };
        self.queue
            .write_buffer(&self.globals_buf, 0, bytemuck::cast_slice(&[globals]));
        self.queue.write_texture(
            self.chunk_texture.as_image_copy(),
            bytemuck::cast_slice(&self.chunks.copy_to_render_buffer(0, 0, 0)),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(lattice::XZ * 4),
                rows_per_image: Some(lattice::XZ),
            },
            self.chunk_texture.size(),
        );

        let out = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(out) => out,
            cst => return Some(cst),
        };
        let out_view = out.texture.create_view(&Default::default());
        let multisample_view = self.multisample_texture.create_view(&Default::default());
        let depth_view = self.depth_texture.create_view(&Default::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("command encoder"),
            });
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("background render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.5,
                            g: 0.0,
                            b: 0.5,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    resolve_target: None,
                    view: &out_view,
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            render_pass.set_pipeline(&self.background_pipeline);
            render_pass.set_bind_group(0, &self.background_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.background_vertices.slice(..));
            render_pass.draw(0..6, 0..1);
        }
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow render pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_map.create_view(&wgpu::TextureViewDescriptor {
                        usage: Some(wgpu::TextureUsages::RENDER_ATTACHMENT),
                        ..Default::default()
                    }),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            render_pass.set_pipeline(&self.shadow_pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
            render_pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint16);

            render_pass.draw_indexed(0..lattice::NUM_INDICES as u32, 0, 0..1);
        }
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Discard,
                    },
                    resolve_target: Some(&out_view),
                    view: &multisample_view,
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_bind_group(1, &self.shadow_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
            render_pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint16);

            render_pass.draw_indexed(0..lattice::NUM_INDICES as u32, 0, 0..1);
        }

        self.queue.submit([encoder.finish()]);
        self.frame_count += 1;
        out.present();
        None
    }

    fn create_depth_texture(
        device: &wgpu::Device,
        surface_config: &wgpu::SurfaceConfiguration,
    ) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth texture"),
            size: wgpu::Extent3d {
                width: surface_config.width,
                height: surface_config.height,
                depth_or_array_layers: 1,
            },
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            dimension: wgpu::TextureDimension::D2,
            sample_count: 4,
            mip_level_count: 1,
            view_formats: &[],
        })
    }

    fn create_multisample_texture(
        device: &wgpu::Device,
        surface_config: &wgpu::SurfaceConfiguration,
    ) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Multisample texture"),
            dimension: wgpu::TextureDimension::D2,
            format: surface_config.format,
            size: wgpu::Extent3d {
                width: surface_config.width,
                height: surface_config.height,
                ..Default::default()
            },
            mip_level_count: 1,
            sample_count: 4,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    }
}
