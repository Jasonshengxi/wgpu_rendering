use bytemuck::{cast_slice, Pod, Zeroable};
use std::marker::PhantomData;
use std::mem;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingType, Buffer, BufferAddress, BufferBindingType, BufferDescriptor,
    BufferUsages, CommandEncoder, Device, Queue, RenderPass, ShaderStages,
};

pub struct DynamicStorageBuffer<I: Zeroable + Pod> {
    length: u32,
    item_capacity: BufferAddress,

    buffer: Buffer,
    layout: BindGroupLayout,
    bind_group: BindGroup,

    phantom_data: PhantomData<I>,
}

impl<I: Zeroable + Pod> DynamicStorageBuffer<I> {
    pub fn new(device: &Device) -> Self {
        Self::with_capacity(device, 4)
    }

    pub fn len(&self) -> u32 {
        self.length
    }

    pub fn bind_group_layout(&self) -> &BindGroupLayout {
        &self.layout
    }

    pub fn with_capacity(device: &Device, item_capacity: BufferAddress) -> Self {
        let byte_capacity = Self::item_to_byte_capacity(item_capacity);
        let buffer = Self::create_buffer(device, byte_capacity, false);
        let bind_group_layout = Self::create_bind_group_layout(device);
        let bind_group = Self::create_bind_group(device, &bind_group_layout, &buffer);

        Self {
            length: 0,
            item_capacity,
            buffer,
            layout: bind_group_layout,
            bind_group,
            phantom_data: PhantomData,
        }
    }

    pub const fn item_to_byte_capacity(item_capacity: BufferAddress) -> BufferAddress {
        item_capacity * (mem::size_of::<I>() as BufferAddress)
    }

    pub fn create_bind_group_layout(device: &Device) -> BindGroupLayout {
        device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("instance bind group layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX_FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    }

    fn create_buffer(device: &Device, size: BufferAddress, mapped_at_creation: bool) -> Buffer {
        device.create_buffer(&BufferDescriptor {
            label: Some("instance buffer"),
            size,
            usage: BufferUsages::union(
                BufferUsages::union(BufferUsages::COPY_SRC, BufferUsages::COPY_DST),
                BufferUsages::STORAGE,
            ),
            mapped_at_creation,
        })
    }

    pub fn shrink_to_fit(&mut self, device: &Device, command_encoder: &mut CommandEncoder) {
        let item_capacity = self.length as BufferAddress;
        let old_buffer = self.replace_buffer_with_new_length(device, item_capacity, false);

        command_encoder.copy_buffer_to_buffer(
            &old_buffer,
            0,
            &self.buffer,
            0,
            Self::item_to_byte_capacity(item_capacity),
        );
    }

    pub fn set_new_data(&mut self, device: &Device, queue: &Queue, data: &[I]) {
        if data.len() <= self.item_capacity as usize {
            queue.write_buffer(&self.buffer, 0, cast_slice(data));
        } else {
            let new_shape_capacity = (data.len() as BufferAddress).next_power_of_two();
            let new_data = cast_slice(data);
            self.replace_buffer_with_new_length(device, new_shape_capacity, true);

            self.buffer.slice(..).get_mapped_range_mut()[..new_data.len()]
                .copy_from_slice(new_data);
            self.buffer.unmap();
        }
        self.length = data.len() as u32;
    }

    pub fn bind_to(&self, render_pass: &mut RenderPass, index: u32) {
        render_pass.set_bind_group(index, &self.bind_group, &[]);
    }

    fn replace_buffer_with_new_length(
        &mut self,
        device: &Device,
        new_item_capacity: BufferAddress,
        mapped_at_creation: bool,
    ) -> Buffer {
        let new_byte_capacity = Self::item_to_byte_capacity(new_item_capacity);

        let new_buffer = Self::create_buffer(device, new_byte_capacity, mapped_at_creation);
        let new_bind_group = Self::create_bind_group(device, &self.layout, &new_buffer);

        let old_buffer = mem::replace(&mut self.buffer, new_buffer);
        self.bind_group = new_bind_group;
        self.item_capacity = new_item_capacity;

        old_buffer
    }

    fn create_bind_group<'a>(
        device: &'a Device,
        layout: &'a BindGroupLayout,
        buffer: &'a Buffer,
    ) -> BindGroup {
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("instance bind group"),
            layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    }
}
