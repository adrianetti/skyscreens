//! Vulkan helpers: uploads BGRA pixels into a region of an image through a staging buffer.
//! The same path will carry lossless tiles from the Mac.

use crate::Result;
use ash::vk;

pub struct Uploader {
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

impl Uploader {
    pub fn new(instance: &ash::Instance, pdev: vk::PhysicalDevice, device: ash::Device, queue_family: u32, capacity: usize) -> Result<Uploader> {
        unsafe {
            let queue = device.get_device_queue(queue_family, 0);
            let pool = device.create_command_pool(
                &vk::CommandPoolCreateInfo::default().queue_family_index(queue_family).flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )?;
            let cmd = device.allocate_command_buffers(&vk::CommandBufferAllocateInfo::default().command_pool(pool).command_buffer_count(1))?[0];
            let fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
            let staging = device.create_buffer(&vk::BufferCreateInfo::default().size(capacity as u64).usage(vk::BufferUsageFlags::TRANSFER_SRC), None)?;
            let req = device.get_buffer_memory_requirements(staging);
            let props = instance.get_physical_device_memory_properties(pdev);
            let wanted = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
            let type_index = (0..props.memory_type_count)
                .find(|&i| req.memory_type_bits & (1 << i) != 0 && props.memory_types[i as usize].property_flags.contains(wanted))
                .ok_or("no host-visible memory")?;
            let memory = device.allocate_memory(&vk::MemoryAllocateInfo::default().allocation_size(req.size).memory_type_index(type_index), None)?;
            device.bind_buffer_memory(staging, memory, 0)?;
            let mapped = device.map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())? as *mut u8;
            Ok(Uploader { device, queue, pool, cmd, fence, staging, memory, mapped, capacity })
        }
    }

    /// Copies tightly packed pixels into `rect` of `image`, then leaves the image ready to display.
    /// `old_layout` must be the image's current layout (UNDEFINED discards its contents).
    pub fn upload(&self, image: vk::Image, old_layout: vk::ImageLayout, rect: vk::Rect2D, pixels: &[u8]) -> Result<()> {
        assert!(pixels.len() <= self.capacity);
        let range = vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).level_count(1).layer_count(1);
        let barrier = |from, to, src, dst| {
            vk::ImageMemoryBarrier::default()
                .image(image)
                .old_layout(from)
                .new_layout(to)
                .src_access_mask(src)
                .dst_access_mask(dst)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .subresource_range(range)
        };
        unsafe {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), self.mapped, pixels.len());
            let d = &self.device;
            d.reset_command_buffer(self.cmd, vk::CommandBufferResetFlags::empty())?;
            d.begin_command_buffer(self.cmd, &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT))?;
            d.cmd_pipeline_barrier(self.cmd, vk::PipelineStageFlags::TOP_OF_PIPE, vk::PipelineStageFlags::TRANSFER, vk::DependencyFlags::empty(), &[], &[], &[barrier(
                old_layout,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::AccessFlags::empty(),
                vk::AccessFlags::TRANSFER_WRITE,
            )]);
            let copy = vk::BufferImageCopy::default()
                .image_subresource(vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).layer_count(1))
                .image_offset(vk::Offset3D { x: rect.offset.x, y: rect.offset.y, z: 0 })
                .image_extent(vk::Extent3D { width: rect.extent.width, height: rect.extent.height, depth: 1 });
            d.cmd_copy_buffer_to_image(self.cmd, self.staging, image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[copy]);
            // OpenXR expects released images in the color attachment layout.
            d.cmd_pipeline_barrier(self.cmd, vk::PipelineStageFlags::TRANSFER, vk::PipelineStageFlags::BOTTOM_OF_PIPE, vk::DependencyFlags::empty(), &[], &[], &[barrier(
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::empty(),
            )]);
            d.end_command_buffer(self.cmd)?;
            d.queue_submit(self.queue, &[vk::SubmitInfo::default().command_buffers(&[self.cmd])], self.fence)?;
            d.wait_for_fences(&[self.fence], true, u64::MAX)?;
            d.reset_fences(&[self.fence])?;
        }
        Ok(())
    }
}

impl Drop for Uploader {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_buffer(self.staging, None);
            self.device.free_memory(self.memory, None);
            self.device.destroy_fence(self.fence, None);
            self.device.destroy_command_pool(self.pool, None);
        }
    }
}
