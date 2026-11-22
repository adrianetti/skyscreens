#![expect(unsafe_code, reason = "Vulkan calls through ash are unsafe")]

use crate::Result;
use crate::runtime::Vulkan;
use ash::vk;

pub(crate) struct Uploader {
    device: ash::Device,
    queue: vk::Queue,
    pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
    fence: vk::Fence,
    staging: vk::Buffer,
    memory: vk::DeviceMemory,
    mapped: *mut u8,
    capacity: usize,
}

struct Staging {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    mapped: *mut u8,
}

impl Uploader {
    pub(crate) fn new(vulkan: &Vulkan, capacity: usize) -> Result<Self> {
        let device = vulkan.device.clone();
        let queue = unsafe { device.get_device_queue(vulkan.queue_family, 0) };
        let pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(vulkan.queue_family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let pool = unsafe { device.create_command_pool(&pool_info, None) }?;
        let cmd_info = vk::CommandBufferAllocateInfo::default().command_pool(pool).command_buffer_count(1);
        let cmd = unsafe { device.allocate_command_buffers(&cmd_info) }?.first().copied().ok_or("no command buffer")?;
        let fence = unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None) }?;
        let staging = Self::staging(vulkan, capacity)?;
        Ok(Self {
            device,
            queue,
            pool,
            cmd,
            fence,
            staging: staging.buffer,
            memory: staging.memory,
            mapped: staging.mapped,
            capacity,
        })
    }

    fn staging(vulkan: &Vulkan, capacity: usize) -> Result<Staging> {
        let device = &vulkan.device;
        let buffer_info =
            vk::BufferCreateInfo::default().size(capacity as u64).usage(vk::BufferUsageFlags::TRANSFER_SRC);
        let buffer = unsafe { device.create_buffer(&buffer_info, None) }?;
        let req = unsafe { device.get_buffer_memory_requirements(buffer) };
        let props = unsafe { vulkan.instance.get_physical_device_memory_properties(vulkan.physical_device) };
        let wanted = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
        let type_index = (0..props.memory_type_count)
            .find(|&i| {
                req.memory_type_bits & (1 << i) != 0 && props.memory_types[i as usize].property_flags.contains(wanted)
            })
            .ok_or("no host-visible memory")?;
        let alloc = vk::MemoryAllocateInfo::default().allocation_size(req.size).memory_type_index(type_index);
        let memory = unsafe { device.allocate_memory(&alloc, None) }?;
        unsafe { device.bind_buffer_memory(buffer, memory, 0) }?;
        let mapped = unsafe { device.map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty()) }?.cast::<u8>();
        Ok(Staging { buffer, memory, mapped })
    }

    pub(crate) fn upload(
        &self,
        image: vk::Image,
        old_layout: vk::ImageLayout,
        rect: vk::Rect2D,
        pixels: &[u8],
    ) -> Result<()> {
        if pixels.len() > self.capacity {
            return Err(format!("{} bytes exceed the {} byte staging buffer", pixels.len(), self.capacity).into());
        }
        unsafe { std::ptr::copy_nonoverlapping(pixels.as_ptr(), self.mapped, pixels.len()) };
        self.record_copy(image, old_layout, rect)?;
        self.submit_and_wait()
    }

    fn record_copy(&self, image: vk::Image, old_layout: vk::ImageLayout, rect: vk::Rect2D) -> Result<()> {
        let d = &self.device;
        let to_transfer = layout_barrier(
            image,
            old_layout,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::AccessFlags::empty(),
            vk::AccessFlags::TRANSFER_WRITE,
        );
        let to_compositor = layout_barrier(
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::empty(),
        );
        let copy = vk::BufferImageCopy::default()
            .image_subresource(
                vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).layer_count(1),
            )
            .image_offset(vk::Offset3D { x: rect.offset.x, y: rect.offset.y, z: 0 })
            .image_extent(vk::Extent3D { width: rect.extent.width, height: rect.extent.height, depth: 1 });
        let begin = vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        let none = vk::DependencyFlags::empty();
        unsafe { d.reset_command_buffer(self.cmd, vk::CommandBufferResetFlags::empty()) }?;
        unsafe { d.begin_command_buffer(self.cmd, &begin) }?;
        let (top, transfer, bottom) = (
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
        );
        unsafe { d.cmd_pipeline_barrier(self.cmd, top, transfer, none, &[], &[], &[to_transfer]) };
        let dst = vk::ImageLayout::TRANSFER_DST_OPTIMAL;
        unsafe { d.cmd_copy_buffer_to_image(self.cmd, self.staging, image, dst, &[copy]) };
        unsafe { d.cmd_pipeline_barrier(self.cmd, transfer, bottom, none, &[], &[], &[to_compositor]) };
        unsafe { d.end_command_buffer(self.cmd) }?;
        Ok(())
    }

    fn submit_and_wait(&self) -> Result<()> {
        let d = &self.device;
        let cmds = [self.cmd];
        let submit = vk::SubmitInfo::default().command_buffers(&cmds);
        unsafe { d.queue_submit(self.queue, &[submit], self.fence) }?;
        unsafe { d.wait_for_fences(&[self.fence], true, u64::MAX) }?;
        unsafe { d.reset_fences(&[self.fence]) }?;
        Ok(())
    }
}

fn layout_barrier(
    image: vk::Image,
    from: vk::ImageLayout,
    to: vk::ImageLayout,
    src: vk::AccessFlags,
    dst: vk::AccessFlags,
) -> vk::ImageMemoryBarrier<'static> {
    vk::ImageMemoryBarrier::default()
        .image(image)
        .old_layout(from)
        .new_layout(to)
        .src_access_mask(src)
        .dst_access_mask(dst)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .subresource_range(
            vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).level_count(1).layer_count(1),
        )
}

impl Drop for Uploader {
    fn drop(&mut self) {
        let d = &self.device;
        let _ = unsafe { d.device_wait_idle() };
        unsafe { d.destroy_buffer(self.staging, None) };
        unsafe { d.free_memory(self.memory, None) };
        unsafe { d.destroy_fence(self.fence, None) };
        unsafe { d.destroy_command_pool(self.pool, None) };
    }
}
