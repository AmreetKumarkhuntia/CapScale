use super::types::{Params, MAX_ZONES, TOGGLES};

impl Params {
    /// Set a named toggle, or flip it when `on` is None. Returns the new state.
    pub fn toggle(&mut self, name: &str, on: Option<bool>) -> Result<bool, String> {
        let slot = match name {
            "upscale" => &mut self.upscale,
            "rcas" => &mut self.rcas,
            "grade" => &mut self.grade,
            "lut" => &mut self.lut,
            "split" => &mut self.split,
            "overlay" => &mut self.overlay,
            "panel" => &mut self.panel,
            "capture" => &mut self.capture,
            "fullscreen" => &mut self.fullscreen,
            "audio" => &mut self.audio,
            "zones" => &mut self.edit_zones,
            _ => {
                return Err(format!(
                    "unknown pass '{name}', expected one of {TOGGLES:?}"
                ))
            }
        };
        *slot = on.unwrap_or(!*slot);
        Ok(*slot)
    }

    /// Keep values inside the ranges the shaders are written for.
    pub fn clamp(&mut self) {
        self.sharpness = self.sharpness.clamp(0.0, 2.0);
        self.exposure = self.exposure.clamp(-5.0, 5.0);
        self.contrast = self.contrast.clamp(0.1, 3.0);
        self.saturation = self.saturation.clamp(0.0, 3.0);
        self.vibrance = self.vibrance.clamp(-1.0, 1.0);
        self.temperature = self.temperature.clamp(-1.0, 1.0);
        self.tint = self.tint.clamp(-1.0, 1.0);
        self.gamma = self.gamma.clamp(0.2, 5.0);
        self.lut_strength = self.lut_strength.clamp(0.0, 1.0);
        self.split_pos = self.split_pos.clamp(0.0, 1.0);
        self.volume = self.volume.clamp(0.0, 2.0);
        self.audio_delay_ms = self.audio_delay_ms.clamp(0.0, 500.0);
        self.content_fps = self.content_fps.clamp(0.0, 240.0);
        self.target_fps = self.target_fps.clamp(0.0, 480.0);
        self.zones.truncate(MAX_ZONES);
        for zone in &mut self.zones {
            let r = zone.rect.map(|v| v.clamp(0.0, 1.0));
            zone.rect = [
                r[0].min(r[2]),
                r[1].min(r[3]),
                r[0].max(r[2]),
                r[1].max(r[3]),
            ];
        }
        for c in 0..3 {
            self.lift[c] = self.lift[c].clamp(-1.0, 1.0);
            self.lgg_gamma[c] = self.lgg_gamma[c].clamp(0.1, 10.0);
            self.gain[c] = self.gain[c].clamp(0.0, 4.0);
        }
    }

    pub fn reset_look(&mut self) {
        let d = Params::default();
        (
            self.sharpness,
            self.exposure,
            self.contrast,
            self.saturation,
            self.vibrance,
        ) = (
            d.sharpness,
            d.exposure,
            d.contrast,
            d.saturation,
            d.vibrance,
        );
        (self.temperature, self.tint, self.gamma, self.lut_strength) =
            (d.temperature, d.tint, d.gamma, d.lut_strength);
        (self.lift, self.lgg_gamma, self.gain) = (d.lift, d.lgg_gamma, d.gain);
    }

    /// The config.toml sections the options panel owns ([output], [params], [passes],
    /// [audio]), holding the current values.
    /// `panel` and `capture` are left out on purpose: they are window state, not settings.
    pub fn to_toml(&self) -> String {
        let f = |v: f32| format!("{v:?}");
        let v3 = |v: [f32; 3]| {
            if v[0] == v[1] && v[1] == v[2] {
                f(v[0])
            } else {
                format!("[{}, {}, {}]", f(v[0]), f(v[1]), f(v[2]))
            }
        };
        let decode = match (self.full_range, self.matrix) {
            (None, None) => {
                "# full_range and matrix are left out: trust what the capture card reports\n"
                    .to_string()
            }
            (range, matrix) => {
                range
                    .map(|r| format!("full_range = {r}\n"))
                    .unwrap_or_default()
                    + &matrix
                        .map(|m| format!("matrix = \"{}\"\n", m.name()))
                        .unwrap_or_default()
            }
        };
        let zones = self
            .zones
            .iter()
            .map(|z| {
                let [left, top, right, bottom] = z.rect.map(f);
                format!(
                    "[{left}, {top}, {right}, {bottom}, {}]",
                    f(z.ellipse as u8 as f32)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "[output]\n\
             width = {}            # 0 x 0: fit the display\n\
             height = {}\n\
             fullscreen = {}\n\
             vsync = \"{}\"    # mailbox | fifo | immediate\n\
             backend = \"{}\"     # auto | dx12 | vulkan; used at the next start\n\
             \n\
             [params]\n\
             upscaler = \"{}\"    # easu | lanczos3 | bilinear\n\
             sharpness = {}      # RCAS, AMD's scale: 0.0 is sharpest, each +1.0 halves it, up to 2.0\n\
             exposure = {}       # stops\n\
             contrast = {}\n\
             saturation = {}\n\
             vibrance = {}\n\
             temperature = {}    # -1 cool .. +1 warm\n\
             tint = {}           # -1 green .. +1 magenta\n\
             gamma = {}\n\
             lift = {}           # a number, or [r, g, b]\n\
             lgg_gamma = {}\n\
             gain = {}\n\
             lut_path = {}       # path to a .cube file\n\
             lut_strength = {}\n\
             split_pos = {}\n\
             content_fps = {}    # the game's own frame rate; 0.0 measures it\n\
             target_fps = {}     # frame generation up to this rate; 0.0 is off\n\
             hud_zones = [{}]    # kept out of frame generation: [left, top, right, bottom, 1.0 if round]\n\
             ai_framegen = {}   # true: frames from the AI model (see README); false: built-in\n\
             ai_still = {}       # AI method: still areas are left as the real picture\n\
             {decode}\
             \n\
             [passes]\n\
             upscale = {}       # false: plain bilinear\n\
             rcas = {}\n\
             grade = {}\n\
             lut = {}\n\
             split = {}        # before / after, drag the divider\n\
             overlay = {}       # fps, latency, per-pass GPU time\n\
             \n\
             [audio]\n\
             enabled = {}\n\
             volume = {}\n\
             delay_ms = {}       # hold the sound back, if it arrives before the picture\n",
            self.out_size.0,
            self.out_size.1,
            self.fullscreen,
            self.vsync.name(),
            self.backend.name(),
            self.upscaler.name(),
            f(self.sharpness),
            f(self.exposure),
            f(self.contrast),
            f(self.saturation),
            f(self.vibrance),
            f(self.temperature),
            f(self.tint),
            f(self.gamma),
            v3(self.lift),
            v3(self.lgg_gamma),
            v3(self.gain),
            format_args!("{:?}", self.lut_path),
            f(self.lut_strength),
            f(self.split_pos),
            f(self.content_fps),
            f(self.target_fps),
            zones,
            self.ai_framegen,
            self.ai_still,
            self.upscale,
            self.rcas,
            self.grade,
            self.lut,
            self.split,
            self.overlay,
            self.audio,
            f(self.volume),
            f(self.audio_delay_ms),
        )
    }
}
