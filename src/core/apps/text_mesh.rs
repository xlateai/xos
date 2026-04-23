use crate::apps::text::text::TextApp;
use crate::auth::{
    is_logged_in, load_identity, load_node_identity, login_offline, reset_offline_identity,
};
use crate::engine::{Application, EngineState};
use crate::mesh::{MeshMode, MeshSession};
use crate::rasterizer::fill_rect_buffer;
use crate::rasterizer::text::fonts;
use crate::rasterizer::text::text_rasterization::TextRasterizer;
use std::sync::Arc;
use std::time::{Duration, Instant};

const STATUS_BAR_HEIGHT: f32 = 44.0;
const INPUT_BOX_HEIGHT: f32 = 46.0;
const INPUT_LEFT_PAD: f32 = 14.0;
const LOGIN_TOP_GAP: f32 = 22.0;
const LOGIN_LINE_GAP: f32 = 14.0;
const DEFAULT_CHANNEL: &str = "shared-text-demo";
const DEFAULT_MODE: MeshMode = MeshMode::Lan;
const DOC_KIND: &str = "shared_doc_v1";

#[derive(Clone, Copy, PartialEq, Eq)]
enum LoginField {
    Username,
    Password,
    Channel,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppPhase {
    Login,
    Editor,
    Error,
}

pub struct TextMeshApp {
    phase: AppPhase,
    active_field: LoginField,
    username: String,
    password: String,
    channel: String,
    mode: MeshMode,
    error_message: String,
    text_app: TextApp,
    mesh_session: Option<Arc<MeshSession>>,
    doc_rev: u64,
    last_sent_text: String,
    last_broadcast_at: Instant,
    status_user_label: String,
    username_button_bounds: (f32, f32, f32, f32),
}

impl TextMeshApp {
    pub fn new() -> Self {
        let mut text_app = TextApp::new();
        text_app.uses_parent_ui_scale = true;
        text_app.show_debug_visuals = false;
        text_app.show_cursor = true;
        let status_user_label = Self::current_username().unwrap_or_else(|| "not logged in".to_string());
        Self {
            phase: if is_logged_in() { AppPhase::Editor } else { AppPhase::Login },
            active_field: LoginField::Username,
            username: status_user_label.clone(),
            password: String::new(),
            channel: DEFAULT_CHANNEL.to_string(),
            mode: DEFAULT_MODE,
            error_message: String::new(),
            text_app,
            mesh_session: None,
            doc_rev: 0,
            last_sent_text: String::new(),
            last_broadcast_at: Instant::now(),
            status_user_label,
            username_button_bounds: (0.0, 0.0, 0.0, 0.0),
        }
    }

    fn current_username() -> Option<String> {
        load_identity().ok().map(|u| u.username).filter(|u| !u.trim().is_empty())
    }

    fn draw_text_line(
        &self,
        state: &mut EngineState,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: (u8, u8, u8),
    ) {
        let mut raster = TextRasterizer::new(fonts::default_font(), font_size);
        raster.set_text(text.to_string());
        let shape = state.frame.shape();
        let width = shape[1] as i32;
        let height = shape[0] as i32;
        let buffer = state.frame_buffer_mut();
        for ch in &raster.characters {
            let px = (x + ch.x) as i32;
            let py = (y + ch.y) as i32;
            for gy in 0..ch.metrics.height {
                for gx in 0..ch.metrics.width {
                    let a = ch.bitmap[gy * ch.metrics.width + gx];
                    if a == 0 {
                        continue;
                    }
                    let sx = px + gx as i32;
                    let sy = py + gy as i32;
                    if sx < 0 || sy < 0 || sx >= width || sy >= height {
                        continue;
                    }
                    let idx = ((sy * width + sx) * 4) as usize;
                    buffer[idx] = ((color.0 as u16 * a as u16) / 255) as u8;
                    buffer[idx + 1] = ((color.1 as u16 * a as u16) / 255) as u8;
                    buffer[idx + 2] = ((color.2 as u16 * a as u16) / 255) as u8;
                    buffer[idx + 3] = 255;
                }
            }
        }
    }

    fn draw_box(&self, state: &mut EngineState, x: i32, y: i32, w: i32, h: i32, rgba: (u8, u8, u8, u8)) {
        let shape = state.frame.shape();
        let fw = shape[1] as usize;
        let fh = shape[0] as usize;
        let buffer = state.frame_buffer_mut();
        fill_rect_buffer(buffer, fw, fh, x, y, x + w, y + h, rgba);
    }

    fn ensure_login_identity(&self) -> Result<(), String> {
        let user = self.username.trim();
        let pass = self.password.trim();
        if !is_logged_in() {
            if user.is_empty() || pass.is_empty() {
                return Err("username and password are required".to_string());
            }
            login_offline(user, pass, user).map_err(|e| format!("login setup failed: {e}"))?;
            return Ok(());
        }

        if pass.is_empty() {
            return Ok(());
        }
        if user.is_empty() {
            return Err("username required when changing login".to_string());
        }
        reset_offline_identity(user, pass, user).map_err(|e| format!("login reset failed: {e}"))?;
        Ok(())
    }

    fn connect_mesh(&mut self) -> Result<(), String> {
        self.ensure_login_identity()?;
        self.status_user_label = Self::current_username().unwrap_or_else(|| "not logged in".to_string());
        let node_identity = load_node_identity().map_err(|e| format!("node identity unavailable: {e}"))?;
        let session = MeshSession::join_with_identity(
            self.channel.trim(),
            self.mode,
            Arc::new(node_identity),
            None,
        )?;
        self.mesh_session = Some(Arc::new(session));
        self.phase = AppPhase::Editor;
        self.last_sent_text = self.text_app.text_rasterizer.text.clone();
        Ok(())
    }

    fn draw_status_bar(&mut self, state: &mut EngineState) {
        let shape = state.frame.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let safe = &state.frame.safe_region_boundaries;
        let top = safe.y1 * height;
        self.draw_box(state, 0, top as i32, width as i32, STATUS_BAR_HEIGHT as i32, (18, 24, 28, 255));

        let clients = self
            .mesh_session
            .as_ref()
            .map(|m| m.current_num_nodes())
            .unwrap_or(1);
        let mode = match self.mode {
            MeshMode::Local => "LOCAL",
            MeshMode::Lan => "LAN",
        };
        let left = format!(
            "● {} | mode {} | clients {} | rev {}",
            self.channel, mode, clients, self.doc_rev
        );
        self.draw_text_line(state, &left, 12.0, top + 10.0, 20.0, (230, 236, 242));

        let user_label = format!("@{}", self.status_user_label);
        let button_w = (user_label.chars().count() as f32 * 11.0 + 28.0).max(120.0);
        let bx = (width - button_w - 12.0).max(12.0);
        let by = top + 6.0;
        self.draw_box(
            state,
            bx as i32,
            by as i32,
            button_w as i32,
            (STATUS_BAR_HEIGHT - 12.0) as i32,
            (42, 56, 68, 255),
        );
        self.draw_text_line(state, &user_label, bx + 10.0, by + 7.0, 18.0, (220, 235, 245));
        self.username_button_bounds = (bx, by, bx + button_w, by + STATUS_BAR_HEIGHT - 12.0);
    }

    fn draw_login_screen(&self, state: &mut EngineState) {
        let shape = state.frame.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        let safe = &state.frame.safe_region_boundaries;
        let top = safe.y1 * height + STATUS_BAR_HEIGHT + LOGIN_TOP_GAP;
        let left = 24.0;
        let content_w = (width - 48.0).max(200.0);

        self.draw_text_line(
            state,
            "Text Mesh Login",
            left,
            top,
            if cfg!(target_os = "ios") { 34.0 } else { 28.0 },
            (240, 240, 240),
        );
        self.draw_text_line(
            state,
            "Use existing login or type new creds then press ENTER.",
            left,
            top + 42.0,
            18.0,
            (160, 170, 180),
        );

        let field_top = top + 84.0;
        let field_w = content_w;
        self.draw_box(
            state,
            left as i32,
            field_top as i32,
            field_w as i32,
            INPUT_BOX_HEIGHT as i32,
            if self.active_field == LoginField::Username {
                (42, 56, 68, 255)
            } else {
                (26, 32, 38, 255)
            },
        );
        self.draw_text_line(
            state,
            &format!("username: {}", self.username),
            left + INPUT_LEFT_PAD,
            field_top + 12.0,
            20.0,
            (240, 240, 240),
        );

        let field2_top = field_top + INPUT_BOX_HEIGHT + LOGIN_LINE_GAP;
        self.draw_box(
            state,
            left as i32,
            field2_top as i32,
            field_w as i32,
            INPUT_BOX_HEIGHT as i32,
            if self.active_field == LoginField::Password {
                (42, 56, 68, 255)
            } else {
                (26, 32, 38, 255)
            },
        );
        let masked = "*".repeat(self.password.chars().count());
        self.draw_text_line(
            state,
            &format!("password: {}", masked),
            left + INPUT_LEFT_PAD,
            field2_top + 12.0,
            20.0,
            (240, 240, 240),
        );

        let field3_top = field2_top + INPUT_BOX_HEIGHT + LOGIN_LINE_GAP;
        self.draw_box(
            state,
            left as i32,
            field3_top as i32,
            field_w as i32,
            INPUT_BOX_HEIGHT as i32,
            if self.active_field == LoginField::Channel {
                (42, 56, 68, 255)
            } else {
                (26, 32, 38, 255)
            },
        );
        self.draw_text_line(
            state,
            &format!("channel: {}", self.channel),
            left + INPUT_LEFT_PAD,
            field3_top + 12.0,
            20.0,
            (240, 240, 240),
        );
        self.draw_text_line(
            state,
            "TAB moves fields. ENTER connects. Click @username to switch account.",
            left,
            field3_top + INPUT_BOX_HEIGHT + 20.0,
            17.0,
            (160, 170, 180),
        );
        if !self.error_message.is_empty() {
            self.draw_text_line(
                state,
                &self.error_message,
                left,
                field3_top + INPUT_BOX_HEIGHT + 48.0,
                18.0,
                (240, 90, 90),
            );
        }
    }

    fn handle_remote_packets(&mut self) {
        let Some(session) = self.mesh_session.as_ref() else {
            return;
        };
        let inbox = session.inbox();
        if let Ok(Some(packets)) = inbox.receive(DOC_KIND, false, true) {
            for packet in packets {
                let rev = packet.body.get("rev").and_then(|v| v.as_u64()).unwrap_or(0);
                let text = packet.body.get("text").and_then(|v| v.as_str()).unwrap_or("");
                if rev >= self.doc_rev {
                    self.doc_rev = rev;
                    self.text_app.text_rasterizer.text = text.to_string();
                    self.text_app.cursor_position = self
                        .text_app
                        .cursor_position
                        .min(self.text_app.text_rasterizer.text.chars().count());
                    self.last_sent_text = self.text_app.text_rasterizer.text.clone();
                }
            }
        }
    }

    fn maybe_broadcast_doc(&mut self) {
        let Some(session) = self.mesh_session.as_ref() else {
            return;
        };
        let now = Instant::now();
        if now.duration_since(self.last_broadcast_at) < Duration::from_millis(120) {
            return;
        }
        let current = self.text_app.text_rasterizer.text.clone();
        if current == self.last_sent_text {
            return;
        }
        self.doc_rev = self.doc_rev.saturating_add(1);
        let payload = serde_json::json!({
            "rev": self.doc_rev,
            "text": current,
            "from_rank": session.rank(),
        });
        if session.broadcast_json(DOC_KIND, payload).is_ok() {
            self.last_broadcast_at = now;
            self.last_sent_text = self.text_app.text_rasterizer.text.clone();
        }
    }

    fn username_button_hit(&self, x: f32, y: f32) -> bool {
        let (x0, y0, x1, y1) = self.username_button_bounds;
        x >= x0 && x <= x1 && y >= y0 && y <= y1
    }
}

impl Application for TextMeshApp {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        self.text_app.setup(state)?;
        if self.phase == AppPhase::Editor {
            if let Err(e) = self.connect_mesh() {
                self.error_message = e;
                self.phase = AppPhase::Login;
            }
        }
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        match self.phase {
            AppPhase::Login | AppPhase::Error => {
                crate::rasterizer::fill(&mut state.frame, (8, 10, 12, 255));
                self.draw_status_bar(state);
                self.draw_login_screen(state);
            }
            AppPhase::Editor => {
                let shape = state.frame.shape();
                let height = shape[0] as f32;
                let safe = &state.frame.safe_region_boundaries;
                let unsafe_bottom = ((1.0 - safe.y2) * height).max(0.0);
                self.text_app.top_chrome_height_px = STATUS_BAR_HEIGHT;
                self.text_app.bottom_chrome_height_px = unsafe_bottom;
                let base_font = if cfg!(target_os = "ios") { 36.0 } else { 26.0 };
                self.text_app.set_font_size(base_font);
                self.handle_remote_packets();
                self.text_app.tick(state);
                self.draw_status_bar(state);
                self.maybe_broadcast_doc();
            }
        }
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        if self.username_button_hit(state.mouse.x, state.mouse.y) {
            self.mesh_session = None;
            self.phase = AppPhase::Login;
            self.password.clear();
            self.error_message.clear();
            self.status_user_label = Self::current_username().unwrap_or_else(|| "not logged in".to_string());
            self.username = self.status_user_label.clone();
            return;
        }
        if self.phase == AppPhase::Editor {
            self.text_app.on_mouse_down(state);
            return;
        }
        let shape = state.frame.shape();
        let height = shape[0] as f32;
        let safe = &state.frame.safe_region_boundaries;
        let top = safe.y1 * height + STATUS_BAR_HEIGHT + LOGIN_TOP_GAP + 84.0;
        let y = state.mouse.y;
        self.active_field = if y < top + INPUT_BOX_HEIGHT {
            LoginField::Username
        } else if y < top + INPUT_BOX_HEIGHT * 2.0 + LOGIN_LINE_GAP {
            LoginField::Password
        } else {
            LoginField::Channel
        };
    }

    fn on_mouse_up(&mut self, state: &mut EngineState) {
        if self.phase == AppPhase::Editor {
            self.text_app.on_mouse_up(state);
        }
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        if self.phase == AppPhase::Editor {
            self.text_app.on_mouse_move(state);
        }
    }

    fn on_scroll(&mut self, state: &mut EngineState, dx: f32, dy: f32) {
        if self.phase == AppPhase::Editor {
            self.text_app.on_scroll(state, dx, dy);
        }
    }

    fn on_key_char(&mut self, state: &mut EngineState, ch: char) {
        if self.phase == AppPhase::Editor {
            self.text_app.on_key_char(state, ch);
            return;
        }
        match ch {
            '\t' => {
                self.active_field = match self.active_field {
                    LoginField::Username => LoginField::Password,
                    LoginField::Password => LoginField::Channel,
                    LoginField::Channel => LoginField::Username,
                };
            }
            '\r' | '\n' => {
                self.error_message.clear();
                match self.connect_mesh() {
                    Ok(()) => {}
                    Err(e) => {
                        self.error_message = e;
                        self.phase = AppPhase::Error;
                    }
                }
            }
            '\u{1b}' => {
                self.error_message.clear();
                self.phase = AppPhase::Login;
            }
            '\u{8}' => match self.active_field {
                LoginField::Username => {
                    self.username.pop();
                }
                LoginField::Password => {
                    self.password.pop();
                }
                LoginField::Channel => {
                    self.channel.pop();
                }
            },
            _ => {
                if ch.is_control() {
                    return;
                }
                match self.active_field {
                    LoginField::Username => self.username.push(ch),
                    LoginField::Password => self.password.push(ch),
                    LoginField::Channel => self.channel.push(ch),
                }
            }
        }
    }
}
