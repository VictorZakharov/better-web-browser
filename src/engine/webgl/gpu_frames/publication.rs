use super::*;

/// A completed frame and independent producer status. Pending is not a request
/// for a synchronous readback; a valid previous bitmap remains on screen.
#[derive(Default)]
pub(crate) struct Export {
    pub(crate) frame: Option<GpuFrame>,
    pub(crate) pending: bool,
    pub(crate) submitted: bool,
    pub(crate) supported: bool,
}

impl Pool {
    fn frame(&self) -> Option<GpuFrame> {
        let slot = &self.slots[self.latest?];
        Some(GpuFrame {
            surface: slot.id,
            handle: slot.surface.handle().0 as usize as u64,
            key: slot.key,
            width: self.width,
            height: self.height,
            adapter_low: self.adapter_low,
            adapter_high: self.adapter_high,
        })
    }

    pub(super) fn complete_ready(&mut self) -> Result<(), String> {
        for index in 0..self.slots.len() {
            let slot = &mut self.slots[index];
            let Some(completion) = slot.completion.as_ref() else {
                continue;
            };
            if !self.copy.poll(completion)? {
                continue;
            }
            // Keep the write mutex until the fence proves that all bytes exist.
            unsafe { slot.surface.mutex.ReleaseSync(completion.key) }
                .map_err(|error| error.to_string())?;
            let completion = slot.completion.take().unwrap();
            slot.key = completion.key;
            slot.sequence = completion.sequence;
            self.copy.delete_completion(completion);
            if self
                .latest
                .is_none_or(|latest| self.slots[latest].sequence < self.slots[index].sequence)
            {
                self.latest = Some(index);
            }
        }
        Ok(())
    }

    pub(super) fn finish_pending(&mut self) -> Result<(), String> {
        if self.slots.iter().any(|slot| slot.completion.is_some()) {
            self.copy.finish_pending()?;
            self.complete_ready()?;
        }
        Ok(())
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        // Resize, loss and document retirement cannot destroy a surface while
        // its private draw still owns the write mutex. This is not a frame wait.
        let current = self.slots.iter().any(|slot| slot.completion.is_some())
            && self.copy.finish_pending().is_ok();
        for slot in &mut self.slots {
            if let Some(completion) = slot.completion.take() {
                if current {
                    self.copy.delete_completion(completion);
                }
                let _ = unsafe { slot.surface.mutex.ReleaseSync(0) };
            }
        }
    }
}

impl WebGl {
    pub(in crate::engine::webgl) fn poll_gpu_frame(&mut self) -> Result<Export, String> {
        let extent = (self.surface.width, self.surface.height);
        if self.gpu_unavailable_extent == Some(extent) {
            return Ok(Export::default());
        }
        let result = self.export_gpu_frame();
        let restored = self.native.make_current();
        if result.is_err() || restored.is_err() {
            self.gpu_unavailable_extent = Some(extent);
            // Retain the pool for synchronous recovery of its newest submission.
        }
        restored?;
        result
    }

    fn export_gpu_frame(&mut self) -> Result<Export, String> {
        if self.options.api != super::super::ApiVersion::Two || self.objects.poisoned {
            return Ok(Export::default());
        }
        self.native.make_current()?;
        if self.gpu_frames.as_ref().is_some_and(|pool| {
            (pool.width, pool.height) != (self.surface.width, self.surface.height)
        }) {
            self.gpu_frames = None;
            self.native.make_current()?;
        }
        if self.gpu_frames.is_none() {
            self.gpu_frames = Some(Pool::new(self)?);
        }
        let pool = self.gpu_frames.as_mut().unwrap();
        pool.complete_ready()?;
        self.native.make_current()?;
        let mut submitted = false;
        if self.surface.presentation_dirty {
            // Check each reusable slot without waiting for an external reader.
            for index in 0..pool.slots.len() {
                if pool.latest == Some(index) || pool.slots[index].completion.is_some() {
                    continue;
                }
                let slot = &mut pool.slots[index];
                let key = slot.key.checked_add(1).ok_or("GPU frame key exhausted")?;
                let sequence = pool
                    .sequence
                    .checked_add(1)
                    .ok_or("GPU frame sequence exhausted")?;
                if !try_acquire(&slot.surface.mutex, 0)?
                    && (slot.key == 0 || !try_acquire(&slot.surface.mutex, slot.key)?)
                {
                    continue;
                }
                let result = (|| {
                    pool.copy.order_copy_after_author(&self.native)?;
                    if let Some(source) = self.surface.presentation_resolve_source() {
                        pool.copy.resolve(
                            source,
                            self.surface.presentation_texture(),
                            pool.width,
                            pool.height,
                        )?;
                        self.surface.mark_presentation_resolved();
                    }
                    pool.copy.copy(
                        &slot.surface,
                        self.surface.presentation_texture(),
                        pool.width,
                        pool.height,
                        self.options.alpha,
                        self.options.premultiplied_alpha,
                    )
                })();
                match result {
                    Ok(handle) => {
                        slot.completion = Some(completion::Completion {
                            handle,
                            key,
                            sequence,
                        });
                        pool.sequence = sequence;
                        pool.copy.order_author_after_copy(&self.native, handle)?;
                        submitted = true;
                        break;
                    }
                    Err(error) => {
                        // A failed fence allocation may follow a successful draw.
                        let _ = pool.copy.finish_pending();
                        let _ = unsafe { slot.surface.mutex.ReleaseSync(0) };
                        return Err(error);
                    }
                }
            }
        }
        Ok(Export {
            frame: pool.frame(),
            pending: pool.slots.iter().any(|slot| slot.completion.is_some())
                || (self.surface.presentation_dirty && !submitted),
            submitted,
            supported: true,
        })
    }

    #[cfg(test)]
    pub(in crate::engine::webgl) fn gpu_frame(&mut self) -> Result<Option<GpuFrame>, String> {
        let export = self.poll_gpu_frame()?;
        if !export.supported || !export.submitted {
            return Ok(None);
        }
        let pool = self.gpu_frames.as_mut().unwrap();
        pool.finish_pending()?;
        let frame = pool.frame();
        self.native.make_current()?;
        Ok(frame)
    }
}
