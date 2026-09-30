//! Bounded, path-free file selections crossing from the privileged browser to a renderer.
//!
//! The user chooses files in browser UI. Only basenames, metadata, and immutable byte
//! snapshots cross this boundary; neither a filesystem path nor an OS handle is a wire field.

use super::{DocumentId, DocumentNodeId, ProtocolError};
use crate::fetch::RequestClient;
use std::fmt;

pub const MAX_FILE_PICKER_FILES: usize = 8;
pub const MAX_FILE_PICKER_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_FILE_PICKER_CHUNK_BYTES: usize = 64 * 1024;
pub const MAX_FILE_PICKER_NAME_BYTES: usize = 255;
pub const MAX_FILE_PICKER_MIME_BYTES: usize = 255;
pub const MAX_FILE_PICKER_ACCEPT_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilePickerRequest {
    pub document: DocumentId,
    pub request_id: u64,
    pub node: DocumentNodeId,
    pub client: RequestClient,
    pub multiple: bool,
    pub accept: String,
}

impl FilePickerRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id == 0 {
            return Err(ProtocolError::InvalidPayload(
                "file picker request identifier",
            ));
        }
        if self.accept.len() > MAX_FILE_PICKER_ACCEPT_BYTES || self.accept.contains('\0') {
            return Err(ProtocolError::InvalidPayload("file picker accept hint"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedFileMetadata {
    pub name: String,
    pub mime_type: String,
    pub last_modified: i64,
    pub size: u32,
}

impl SelectedFileMetadata {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.name.is_empty()
            || self.name.len() > MAX_FILE_PICKER_NAME_BYTES
            || self.name == "."
            || self.name == ".."
            || self.name.chars().any(|character| {
                character == '/'
                    || character == '\\'
                    || character == ':'
                    || character.is_ascii_control()
            })
        {
            return Err(ProtocolError::InvalidPayload("selected file basename"));
        }
        if self.mime_type.len() > MAX_FILE_PICKER_MIME_BYTES
            || !self.mime_type.is_empty()
                && !self
                    .mime_type
                    .split_once('/')
                    .is_some_and(|(type_, subtype)| {
                        !type_.is_empty()
                            && !subtype.is_empty()
                            && type_.bytes().all(is_mime_token)
                            && subtype.bytes().all(is_mime_token)
                    })
            || self.last_modified.unsigned_abs() > 9_007_199_254_740_991
            || self.size as usize > MAX_FILE_PICKER_BYTES
        {
            return Err(ProtocolError::InvalidPayload("selected file metadata"));
        }
        Ok(())
    }
}

fn is_mime_token(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

#[derive(Clone, PartialEq, Eq)]
pub enum FilePickerUpdate {
    Start {
        document: DocumentId,
        request_id: u64,
        files: Vec<SelectedFileMetadata>,
    },
    Chunk {
        document: DocumentId,
        request_id: u64,
        file_index: u8,
        offset: u32,
        bytes: Vec<u8>,
    },
    End {
        document: DocumentId,
        request_id: u64,
    },
    Canceled {
        document: DocumentId,
        request_id: u64,
    },
    Failed {
        document: DocumentId,
        request_id: u64,
    },
}

impl fmt::Debug for FilePickerUpdate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start {
                document,
                request_id,
                files,
            } => f
                .debug_struct("FilePickerStart")
                .field("document", document)
                .field("request_id", request_id)
                .field("files", files)
                .finish(),
            Self::Chunk {
                document,
                request_id,
                file_index,
                offset,
                bytes,
            } => f
                .debug_struct("FilePickerChunk")
                .field("document", document)
                .field("request_id", request_id)
                .field("file_index", file_index)
                .field("offset", offset)
                .field("byte_count", &bytes.len())
                .finish(),
            Self::End {
                document,
                request_id,
            } => f
                .debug_struct("FilePickerEnd")
                .field("document", document)
                .field("request_id", request_id)
                .finish(),
            Self::Canceled {
                document,
                request_id,
            } => f
                .debug_struct("FilePickerCanceled")
                .field("document", document)
                .field("request_id", request_id)
                .finish(),
            Self::Failed {
                document,
                request_id,
            } => f
                .debug_struct("FilePickerFailed")
                .field("document", document)
                .field("request_id", request_id)
                .finish(),
        }
    }
}

impl FilePickerUpdate {
    pub const fn document(&self) -> DocumentId {
        match self {
            Self::Start { document, .. }
            | Self::Chunk { document, .. }
            | Self::End { document, .. }
            | Self::Canceled { document, .. }
            | Self::Failed { document, .. } => *document,
        }
    }

    pub const fn request_id(&self) -> u64 {
        match self {
            Self::Start { request_id, .. }
            | Self::Chunk { request_id, .. }
            | Self::End { request_id, .. }
            | Self::Canceled { request_id, .. }
            | Self::Failed { request_id, .. } => *request_id,
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id() == 0 {
            return Err(ProtocolError::InvalidPayload(
                "file picker update identifier",
            ));
        }
        match self {
            Self::Start { files, .. } => {
                if files.is_empty() || files.len() > MAX_FILE_PICKER_FILES {
                    return Err(ProtocolError::InvalidPayload("selected file count"));
                }
                let mut total = 0usize;
                for file in files {
                    file.validate()?;
                    total = total
                        .checked_add(file.size as usize)
                        .ok_or(ProtocolError::InvalidPayload("selected file size"))?;
                }
                if total > MAX_FILE_PICKER_BYTES {
                    return Err(ProtocolError::InvalidPayload("selected file size"));
                }
            }
            Self::Chunk {
                file_index, bytes, ..
            } => {
                if usize::from(*file_index) >= MAX_FILE_PICKER_FILES
                    || bytes.is_empty()
                    || bytes.len() > MAX_FILE_PICKER_CHUNK_BYTES
                {
                    return Err(ProtocolError::InvalidPayload("selected file chunk"));
                }
            }
            Self::End { .. } | Self::Canceled { .. } | Self::Failed { .. } => {}
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SelectedFile {
    pub metadata: SelectedFileMetadata,
    pub bytes: Vec<u8>,
}

impl fmt::Debug for SelectedFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SelectedFile")
            .field("metadata", &self.metadata)
            .field("byte_count", &self.bytes.len())
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilePickerSelection {
    Selected(Vec<SelectedFile>),
    Canceled,
    Failed,
}

/// One pending picker reply. Rejects gaps, overlap, reordering, and truncated files.
pub struct FileSelectionAssembler {
    document: DocumentId,
    request_id: u64,
    files: Option<Vec<SelectedFile>>,
    next_file: usize,
    finished: bool,
}

impl FileSelectionAssembler {
    pub fn new(document: DocumentId, request_id: u64) -> Self {
        Self {
            document,
            request_id,
            files: None,
            next_file: 0,
            finished: false,
        }
    }

    pub fn push(
        &mut self,
        update: FilePickerUpdate,
    ) -> Result<Option<FilePickerSelection>, ProtocolError> {
        let result = self.push_inner(update);
        if result.is_err() {
            // A malformed or stale stream cannot be repaired into a trusted FileList.
            self.files = None;
            self.finished = true;
        }
        result
    }

    fn push_inner(
        &mut self,
        update: FilePickerUpdate,
    ) -> Result<Option<FilePickerSelection>, ProtocolError> {
        update.validate()?;
        if self.finished
            || update.document() != self.document
            || update.request_id() != self.request_id
        {
            return Err(ProtocolError::InvalidPayload("file picker reply identity"));
        }
        match update {
            FilePickerUpdate::Start { files, .. } if self.files.is_none() => {
                self.files = Some(
                    files
                        .into_iter()
                        .map(|metadata| SelectedFile {
                            metadata,
                            bytes: Vec::new(),
                        })
                        .collect(),
                );
                self.skip_empty_files();
                Ok(None)
            }
            FilePickerUpdate::Chunk {
                file_index,
                offset,
                bytes,
                ..
            } => {
                let Some(files) = self.files.as_mut() else {
                    return Err(ProtocolError::InvalidPayload("file chunk before start"));
                };
                if usize::from(file_index) != self.next_file || self.next_file >= files.len() {
                    return Err(ProtocolError::InvalidPayload("file chunk order"));
                }
                let file = &mut files[self.next_file];
                if file.bytes.len() != offset as usize
                    || file.bytes.len().saturating_add(bytes.len()) > file.metadata.size as usize
                {
                    return Err(ProtocolError::InvalidPayload("file chunk offset or size"));
                }
                file.bytes.extend_from_slice(&bytes);
                self.skip_empty_files();
                Ok(None)
            }
            FilePickerUpdate::End { .. } => {
                let Some(files) = self.files.take() else {
                    return Err(ProtocolError::InvalidPayload("file end before start"));
                };
                if self.next_file != files.len() {
                    return Err(ProtocolError::InvalidPayload("truncated selected file"));
                }
                self.finished = true;
                Ok(Some(FilePickerSelection::Selected(files)))
            }
            FilePickerUpdate::Canceled { .. } if self.files.is_none() => {
                self.finished = true;
                Ok(Some(FilePickerSelection::Canceled))
            }
            FilePickerUpdate::Failed { .. } => {
                self.files = None;
                self.finished = true;
                Ok(Some(FilePickerSelection::Failed))
            }
            _ => Err(ProtocolError::InvalidPayload("file picker reply order")),
        }
    }

    fn skip_empty_files(&mut self) {
        let Some(files) = &self.files else { return };
        while self.next_file < files.len()
            && files[self.next_file].bytes.len() == files[self.next_file].metadata.size as usize
        {
            self.next_file += 1;
        }
    }
}

#[cfg(test)]
mod tests;
