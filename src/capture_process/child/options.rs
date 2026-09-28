use super::*;

impl ChildOptions {
    pub(super) fn parse(arguments: &[String]) -> Result<Self, String> {
        let mut nonce = None;
        let mut session = None;
        let mut sample_handle = None;
        let mut devices = None;
        let mut test_mode = false;
        let mut iter = arguments.iter();
        while let Some(argument) = iter.next() {
            let value = match argument.as_str() {
                "--capture-process" => continue,
                "--capture-test-mode" => {
                    test_mode = true;
                    continue;
                }
                "--capture-nonce"
                | "--capture-session"
                | "--capture-sample-handle"
                | "--capture-devices" => iter
                    .next()
                    .ok_or_else(|| format!("missing value for {argument}"))?,
                _ => return Err(format!("unknown capture child option: {argument}")),
            };
            match argument.as_str() {
                "--capture-nonce" => {
                    nonce = Some(Nonce::from_hex(value).map_err(|error| error.to_string())?)
                }
                "--capture-session" => {
                    session = Some(
                        CaptureSessionId::new(
                            value
                                .parse::<u64>()
                                .map_err(|_| "invalid capture session")?,
                        )
                        .map_err(|error| error.to_string())?,
                    )
                }
                "--capture-sample-handle" => {
                    sample_handle = Some(
                        value
                            .parse::<usize>()
                            .map_err(|_| "invalid capture sample handle")?,
                    )
                }
                "--capture-devices" => {
                    devices = Some(
                        CaptureDevices::from_bits(
                            value.parse::<u8>().map_err(|_| "invalid capture devices")?,
                        )
                        .map_err(|error| error.to_string())?,
                    )
                }
                _ => unreachable!(),
            }
        }
        Ok(Self {
            nonce: nonce.ok_or("missing capture nonce")?,
            session: session.ok_or("missing capture session")?,
            sample_handle: sample_handle.ok_or("missing capture sample handle")?,
            devices: devices.ok_or("missing capture device grant")?,
            test_mode,
        })
    }
}
