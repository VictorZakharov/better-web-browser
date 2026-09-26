//! SAPI 5 adapter confined to the browser's dedicated COM thread.

use better_web_browser::renderer_protocol::{SpeechAction, SpeechRequest, SpeechVoiceInfo};
use windows::Win32::Globalization::LCIDToLocaleName;
use windows::Win32::Media::Speech::{
    ISpObjectToken, ISpObjectTokenCategory, ISpVoice, SPCAT_VOICES, SPF_ASYNC, SPF_IS_NOT_XML,
    SPF_IS_XML, SPF_PURGEBEFORESPEAK, SPVOICESTATUS, SpObjectTokenCategory, SpVoice,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::core::{PCWSTR, PWSTR, w};

struct Apartment;

impl Apartment {
    fn enter() -> Result<Self, String> {
        let status = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        status
            .ok()
            .map(|()| Self)
            .map_err(|error| format!("initialize speech COM apartment: {error}"))
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

struct InstalledVoice {
    token: ISpObjectToken,
    info: SpeechVoiceInfo,
}

pub(super) struct SapiVoice {
    voice: ISpVoice,
    installed: Vec<InstalledVoice>,
    // Declare last so every COM object is released before apartment teardown.
    _apartment: Apartment,
}

impl SapiVoice {
    pub(super) fn open() -> Result<Self, String> {
        let apartment = Apartment::enter()?;
        let voice: ISpVoice = unsafe { CoCreateInstance(&SpVoice, None, CLSCTX_ALL) }
            .map_err(|error| format!("create SAPI voice: {error}"))?;
        let installed = enumerate_voices(&voice)?;
        Ok(Self {
            voice,
            installed,
            _apartment: apartment,
        })
    }

    pub(super) fn voices(&self) -> Vec<SpeechVoiceInfo> {
        self.installed
            .iter()
            .map(|item| item.info.clone())
            .collect()
    }

    pub(super) fn start(&mut self, request: &SpeechRequest) -> Result<(), String> {
        let SpeechAction::Speak {
            text,
            voice_uri,
            lang,
            rate,
            pitch,
            volume,
        } = &request.action
        else {
            return Err("speech start requires an utterance".into());
        };
        let token = self
            .installed
            .iter()
            .find(|installed| !voice_uri.is_empty() && installed.info.voice_uri == *voice_uri)
            .or_else(|| {
                self.installed.iter().find(|installed| {
                    !lang.is_empty() && installed.info.lang.eq_ignore_ascii_case(lang)
                })
            })
            .or_else(|| {
                self.installed
                    .iter()
                    .find(|installed| installed.info.is_default)
            })
            .or_else(|| self.installed.first())
            .ok_or_else(|| "no installed speech voices".to_string())?;
        unsafe {
            self.voice
                .SetVoice(&token.token)
                .map_err(|error| format!("select SAPI voice: {error}"))?;
            // SAPI's rate scale is logarithmic-ish; map Web Speech's 1.0 to zero.
            self.voice
                .SetRate((rate.log2() * 3.0).round().clamp(-10.0, 10.0) as i32)
                .map_err(|error| format!("set SAPI rate: {error}"))?;
            self.voice
                .SetVolume((volume * 100.0).round().clamp(0.0, 100.0) as u16)
                .map_err(|error| format!("set SAPI volume: {error}"))?;
        }
        // SAPI's XML pitch instruction changes generated audio, not just JS metadata.
        // Escape all page text before inserting it into the native markup document.
        let pitch = ((pitch - 1.0) * 10.0).round().clamp(-10.0, 10.0) as i32;
        let markup = format!("<pitch absmiddle=\"{pitch}\">{}</pitch>", escape_xml(text));
        let wide = nul_terminated(&markup);
        unsafe {
            self.voice
                .Speak(
                    PCWSTR(wide.as_ptr()),
                    (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0 | SPF_IS_XML.0) as u32,
                    None,
                )
                .map_err(|error| format!("start SAPI speech: {error}"))
        }
    }

    pub(super) fn pause(&mut self) -> Result<(), String> {
        unsafe { self.voice.Pause() }.map_err(|error| format!("pause SAPI speech: {error}"))
    }

    pub(super) fn resume(&mut self) -> Result<(), String> {
        unsafe { self.voice.Resume() }.map_err(|error| format!("resume SAPI speech: {error}"))
    }

    pub(super) fn cancel(&mut self) -> Result<(), String> {
        unsafe {
            self.voice.Speak(
                w!(""),
                (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0 | SPF_IS_NOT_XML.0) as u32,
                None,
            )
        }
        .map_err(|error| format!("cancel SAPI speech: {error}"))
    }

    pub(super) fn is_done(&self) -> Result<bool, String> {
        // SAPI reports S_FALSE for a pending asynchronous utterance. The high-
        // level binding erases that distinction, so read the HRESULT directly.
        // GetStatus is not a completion signal for stream/file outputs.
        // https://learn.microsoft.com/en-us/previous-versions/windows/desktop/ms719834(v=vs.85)
        let result = unsafe {
            (windows::core::Interface::vtable(&self.voice).WaitUntilDone)(
                windows::core::Interface::as_raw(&self.voice),
                0,
            )
        };
        result
            .ok()
            .map_err(|error| format!("poll SAPI speech completion: {error}"))?;
        if result.0 != 0 {
            return Ok(false);
        }
        let mut status = SPVOICESTATUS::default();
        unsafe { self.voice.GetStatus(&mut status, std::ptr::null_mut()) }
            .map_err(|error| format!("query SAPI speech status: {error}"))?;
        if status.hrLastResult.is_err() {
            return Err(format!("SAPI voice failed: {}", status.hrLastResult));
        }
        Ok(true)
    }
}

fn enumerate_voices(voice: &ISpVoice) -> Result<Vec<InstalledVoice>, String> {
    let category: ISpObjectTokenCategory =
        unsafe { CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_ALL) }
            .map_err(|error| format!("open SAPI voice category: {error}"))?;
    unsafe { category.SetId(SPCAT_VOICES, false) }
        .map_err(|error| format!("select SAPI voice category: {error}"))?;
    let tokens = unsafe { category.EnumTokens(PCWSTR::null(), PCWSTR::null()) }
        .map_err(|error| format!("enumerate SAPI voices: {error}"))?;
    let mut count = 0;
    unsafe { tokens.GetCount(&mut count) }
        .map_err(|error| format!("count SAPI voices: {error}"))?;
    let default_id = unsafe { voice.GetVoice() }
        .ok()
        .and_then(|token| token_id(&token).ok());
    let mut installed = Vec::new();
    for index in 0..count.min(256) {
        if installed.len() >= 64 {
            break;
        }
        let token = unsafe { tokens.Item(index) }
            .map_err(|error| format!("read SAPI voice {index}: {error}"))?;
        let voice_uri = token_id(&token)?;
        // Keep the opaque token identifier intact for SetVoice. An oversized
        // URI cannot cross the bounded renderer protocol, so skip that voice.
        if voice_uri.is_empty() || voice_uri.len() > 256 {
            continue;
        }
        let attributes = unsafe { token.OpenKey(w!("Attributes")) }.ok();
        let name = attributes
            .as_ref()
            .and_then(|key| unsafe { key.GetStringValue(w!("Name")) }.ok())
            .map(owned_wide_string)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| voice_uri.rsplit('\\').next().unwrap_or("Voice").into());
        let lang = attributes
            .as_ref()
            .and_then(|key| unsafe { key.GetStringValue(w!("Language")) }.ok())
            .map(owned_wide_string)
            .and_then(|value| language_tag(&value))
            .unwrap_or_default();
        installed.push(InstalledVoice {
            info: SpeechVoiceInfo {
                is_default: default_id.as_deref() == Some(&voice_uri),
                voice_uri,
                name: super::bounded_utf8(name, 256),
                lang: super::bounded_utf8(lang, 256),
            },
            token,
        });
    }
    Ok(installed)
}

fn token_id(token: &ISpObjectToken) -> Result<String, String> {
    let pointer =
        unsafe { token.GetId() }.map_err(|error| format!("read SAPI token identifier: {error}"))?;
    Ok(owned_wide_string(pointer))
}

fn owned_wide_string(pointer: PWSTR) -> String {
    let value = unsafe { pointer.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(pointer.0.cast())) };
    value
}

fn language_tag(value: &str) -> Option<String> {
    let first = value.split(';').next()?;
    let lcid = u32::from_str_radix(first.trim(), 16).ok()?;
    let mut name = [0_u16; 85];
    let length = unsafe { LCIDToLocaleName(lcid, Some(&mut name), 0) };
    (length > 1).then(|| String::from_utf16_lossy(&name[..length as usize - 1]))
}

fn nul_terminated(text: &str) -> Vec<u16> {
    text.encode_utf16()
        .filter(|unit| *unit != 0)
        .chain([0])
        .collect()
}

fn escape_xml(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&apos;"),
            '\0' => {}
            _ => result.push(ch),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_text_cannot_inject_sapi_markup() {
        assert_eq!(
            escape_xml("<pitch> & ' \""),
            "&lt;pitch&gt; &amp; &apos; &quot;"
        );
        assert_eq!(nul_terminated("a\0b"), vec![b'a' as u16, b'b' as u16, 0]);
    }

    #[test]
    fn installed_voice_metadata_is_available_without_audio() {
        let Ok(sapi) = SapiVoice::open() else {
            // Server images without desktop speech components still run the pure tests.
            return;
        };
        for voice in sapi.voices() {
            assert!(!voice.voice_uri.is_empty());
            assert!(!voice.name.is_empty());
            assert!(voice.voice_uri.len() <= 256);
            assert!(voice.name.len() <= 256);
            assert!(voice.lang.len() <= 256);
        }
    }

    #[test]
    fn native_voice_generates_waveform_to_file_without_playing_audio() {
        use windows::Win32::Media::Speech::{ISpStream, SPFM_CREATE_ALWAYS, SpFileStream};

        let Ok(mut sapi) = SapiVoice::open() else {
            return;
        };
        if sapi.installed.is_empty() {
            return;
        }
        let path = std::env::temp_dir().join(format!(
            "breeze-speech-test-{}-{}.wav",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let filename = nul_terminated(path.to_string_lossy().as_ref());
        let stream: ISpStream =
            unsafe { CoCreateInstance(&SpFileStream, None, CLSCTX_ALL) }.expect("SAPI file stream");
        unsafe {
            // Use the native output stream's supported PCM format. SAPI rejects
            // a newly bound file stream without a negotiated WAVEFORMATEX.
            let output = sapi.voice.GetOutputStream().expect("native speech output");
            let mut format_id = windows::core::GUID::zeroed();
            let format = output
                // The generated binding marks this pointer const although SAPI
                // writes the selected format identifier through it.
                .GetFormat(std::ptr::addr_of_mut!(format_id))
                .expect("native speech output format");
            let bound = stream.BindToFile(
                PCWSTR(filename.as_ptr()),
                SPFM_CREATE_ALWAYS,
                Some(&format_id),
                Some(format),
                0,
            );
            CoTaskMemFree(Some(format.cast()));
            bound.expect("bind muted output stream");
            sapi.voice
                .SetOutput(&stream, true)
                .expect("redirect SAPI away from speakers");
        }
        let mut request = SpeechRequest {
            document: better_web_browser::renderer_protocol::DocumentId::new(1).unwrap(),
            utterance_id: 1,
            action: SpeechAction::Speak {
                text: "Breeze".into(),
                voice_uri: String::new(),
                lang: String::new(),
                rate: 1.0,
                pitch: 1.0,
                volume: 1.0,
            },
        };
        sapi.start(&request)
            .expect("synthesize through browser native path");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !sapi.is_done().expect("native speech status") {
            assert!(
                std::time::Instant::now() < deadline,
                "SAPI synthesis timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if let SpeechAction::Speak { text, .. } = &mut request.action {
            *text = "a longer phrase to exercise native pause resume and cancel ".repeat(20);
        }
        sapi.start(&request)
            .expect("start cancellable native speech");
        sapi.pause().expect("pause native speech");
        sapi.resume().expect("resume native speech");
        sapi.cancel().expect("cancel native speech");
        let cancellation_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !sapi.is_done().expect("native cancellation status") {
            assert!(
                std::time::Instant::now() < cancellation_deadline,
                "SAPI cancellation timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        unsafe { stream.Close().expect("flush synthesized waveform") };
        let bytes = std::fs::read(&path).expect("read synthesized waveform");
        std::fs::remove_file(&path).expect("remove own temporary waveform");
        assert!(bytes.starts_with(b"RIFF"), "expected WAV header");
        assert!(bytes.len() > 44, "expected nonempty synthesized PCM");
    }
}
