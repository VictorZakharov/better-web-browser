//! Lossless private coverage storage. Omitted bytes are exactly zero, not a
//! reduced-resolution raster. Dense masks keep a direct contiguous layout.

#[derive(Clone)]
struct Span {
    start: usize,
    offset: usize,
    length: usize,
}

#[derive(Clone)]
enum Storage {
    Dense(Box<[u8]>),
    Sparse {
        spans: Box<[Span]>,
        samples: Box<[u8]>,
    },
}

#[derive(Clone)]
pub(super) struct Coverage {
    length: usize,
    storage: Storage,
}

impl Coverage {
    pub(super) fn new(bytes: Vec<u8>) -> Self {
        let length = bytes.len();
        // Small masks cannot amortize span metadata or classification. Keep
        // dense masks dense too: compression must save at least one quarter.
        if length < 1024 {
            return Self::dense(bytes);
        }
        let budget = length - length / 4;
        let mut spans = Vec::new();
        let mut samples = Vec::new();
        let mut cursor = 0;
        while cursor < length {
            if bytes[cursor] == 0 {
                cursor += 1;
                continue;
            }
            let start = cursor;
            while cursor < length && bytes[cursor] != 0 {
                cursor += 1;
            }
            let run = cursor - start;
            // Bound temporary storage before growing either vector. Rich
            // masks fall back without retaining a second full bitmap copy.
            let needed = (spans.len() + 1) * std::mem::size_of::<Span>() + samples.len() + run;
            if needed > budget {
                return Self::dense(bytes);
            }
            spans.push(Span {
                start,
                offset: samples.len(),
                length: run,
            });
            samples.extend_from_slice(&bytes[start..cursor]);
        }
        Self {
            length,
            storage: Storage::Sparse {
                spans: spans.into_boxed_slice(),
                samples: samples.into_boxed_slice(),
            },
        }
    }

    fn dense(bytes: Vec<u8>) -> Self {
        Self {
            length: bytes.len(),
            storage: Storage::Dense(bytes.into_boxed_slice()),
        }
    }

    pub(super) fn len(&self) -> usize {
        self.length
    }

    /// Actual retained payload storage, including span records. Boxed slices
    /// have no unaccounted vector capacity. The cache separately counts keys.
    pub(super) fn bytes(&self) -> usize {
        match &self.storage {
            Storage::Dense(bytes) => bytes.len(),
            Storage::Sparse { spans, samples } => {
                std::mem::size_of_val(spans.as_ref()) + samples.len()
            }
        }
    }

    /// Contiguous covered runs in original index order. Dense regions include
    /// zeros; consumers retain their normal coverage-zero checks in both cases.
    pub(super) fn runs(&self, mut visit: impl FnMut(usize, &[u8])) {
        match &self.storage {
            Storage::Dense(bytes) => visit(0, bytes),
            Storage::Sparse { spans, samples } => {
                for span in spans {
                    visit(span.start, &samples[span.offset..span.offset + span.length]);
                }
            }
        }
    }

    #[cfg(test)]
    pub(super) fn to_vec(&self) -> Vec<u8> {
        match &self.storage {
            Storage::Dense(bytes) => bytes.to_vec(),
            Storage::Sparse { .. } => {
                let mut bytes = vec![0; self.length];
                self.runs(|start, run| bytes[start..start + run.len()].copy_from_slice(run));
                bytes
            }
        }
    }

    pub(super) fn into_vec(self) -> Vec<u8> {
        match self.storage {
            Storage::Dense(bytes) => bytes.into_vec(),
            Storage::Sparse { spans, samples } => {
                let mut bytes = vec![0; self.length];
                for span in spans {
                    bytes[span.start..span.start + span.length]
                        .copy_from_slice(&samples[span.offset..span.offset + span.length]);
                }
                bytes
            }
        }
    }
}

#[cfg(test)]
mod tests;
