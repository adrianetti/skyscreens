use crate::Result;
use crate::runtime::Vulkan;
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
    pub fn new(vulkan: &Vulkan, capacity: usize) -> Result<Uploader> {
        let device = vulkan.device.clone();
        unsafe {
            let queue = device.get_device_queue(vulkan.queue_family, 0);
            let pool = device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(vulkan.queue_family)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )?;
            let cmd = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default().command_pool(pool).command_buffer_count(1),
                )?
                .first()
                .copied()
                .ok_or("no command buffer")?;
            let fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
            let staging = device.create_buffer(
                &vk::BufferCreateInfo::default().size(capacity as u64).usage(vk::BufferUsageFlags::TRANSFER_SRC),
                None,
            )?;
            let req = device.get_buffer_memory_requirements(staging);
            let props = vulkan.instance.get_physical_device_memory_properties(vulkan.physical_device);
            let wanted = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
            let type_index = (0..props.memory_type_count)
                .find(|&i| {
                    req.memory_type_bits & (1 << i) != 0
                        && props.memory_types[i as usize].property_flags.contains(wanted)
                })
                .ok_or("no host-visible memory")?;
            let memory = device.allocate_memory(
                &vk::MemoryAllocateInfo::default().allocation_size(req.size).memory_type_index(type_index),
                None,
            )?;
            device.bind_buffer_memory(staging, memory, 0)?;
            let mapped = device.map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())?.cast::<u8>();
            Ok(Uploader { device, queue, pool, cmd, fence, staging, memory, mapped, capacity })
        }
    }

    pub fn upload(&self, image: vk::Image, old_layout: vk::ImageLayout, rect: vk::Rect2D, pixels: &[u8]) -> Result<()> {
        if pixels.len() > self.capacity {
            return Err(format!("{} bytes exceed the {} byte staging buffer", pixels.len(), self.capacity).into());
        }
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
        let d = &self.device;
        unsafe {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), self.mapped, pixels.len());
            d.reset_command_buffer(self.cmd, vk::CommandBufferResetFlags::empty())?;
            d.begin_command_buffer(
                self.cmd,
                &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
            d.cmd_pipeline_barrier(
                self.cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_transfer],
            );
            d.cmd_copy_buffer_to_image(self.cmd, self.staging, image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[copy]);
            d.cmd_pipeline_barrier(
                self.cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_compositor],
            );
            d.end_command_buffer(self.cmd)?;
            d.queue_submit(self.queue, &[vk::SubmitInfo::default().command_buffers(&[self.cmd])], self.fence)?;
            d.wait_for_fences(&[self.fence], true, u64::MAX)?;
            d.reset_fences(&[self.fence])?;
        }
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
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_buffer(self.staging, None);
            self.device.free_memory(self.memory, None);
            self.device.destroy_fence(self.fence, None);
            self.device.destroy_command_pool(self.pool, None);
        }
    }
}
