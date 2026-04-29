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
// const DEFAULT_MODE: MeshMode = MeshMode::Lan;
const DEFAULT_MODE: MeshMode = MeshMode::Online;
const DOC_KIND: &str = "shared_doc_v1";
const HOST_ANTI_ENTROPY_INTERVAL: Duration = Duration::from_millis(1800);

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
    last_writer_rank: u32,
    observed_nodes: u32,
    last_sent_text: String,
    last_sent_cursor: usize,
    last_sent_selection_start: Option<usize>,
    last_sent_selection_end: Option<usize>,
    last_broadcast_at: Instant,
    last_host_anti_entropy_at: Instant,
    status_user_label: String,
    username_button_bounds: (f32, f32, f32, f32),
    login_done_button_bounds: (f32, f32, f32, f32),
    login_cancel_button_bounds: (f32, f32, f32, f32),
}

impl TextMeshApp {
    pub fn new() -> Self {
        let mut text_app = TextApp::new();
        text_app.uses_parent_ui_scale = true;
        text_app.show_debug_visuals = false;
        text_app.show_cursor = true;
        let current_user = Self::current_username();
        let status_user_label = current_user
            .clone()
            .unwrap_or_else(|| "not logged in".to_string());
        Self {
            phase: if is_logged_in() { AppPhase::Editor } else { AppPhase::Login },
            active_field: LoginField::Username,
            username: current_user.unwrap_or_default(),
            password: String::new(),
            channel: DEFAULT_CHANNEL.to_string(),
            mode: DEFAULT_MODE,
            error_message: String::new(),
            text_app,
            mesh_session: None,
            doc_rev: 0,
            last_writer_rank: 0,
            observed_nodes: 1,
            last_sent_text: String::new(),
            last_sent_cursor: 0,
            last_sent_selection_start: None,
            last_sent_selection_end: None,
            last_broadcast_at: Instant::now(),
            last_host_anti_entropy_at: Instant::now(),
            status_user_label,
            username_button_bounds: (0.0, 0.0, 0.0, 0.0),
            login_done_button_bounds: (0.0, 0.0, 0.0, 0.0),
            login_cancel_button_bounds: (0.0, 0.0, 0.0, 0.0),
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
        raster.tick(width as f32, height as f32);
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
        let (cursor, sel_start, sel_end) = self.text_app.shared_selection_state();
        self.last_sent_cursor = cursor;
        self.last_sent_selection_start = sel_start;
        self.last_sent_selection_end = sel_end;
        self.last_writer_rank = self
            .mesh_session
            .as_ref()
            .map(|s| s.rank())
            .unwrap_or(0);
        self.observed_nodes = self
            .mesh_session
            .as_ref()
            .map(|s| s.current_num_nodes())
            .unwrap_or(1);
        self.last_host_anti_entropy_at = Instant::now();
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
            MeshMode::Online => "ONLINE",
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

    fn draw_login_screen(&mut self, state: &mut EngineState) {
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
        let button_y = field3_top + INPUT_BOX_HEIGHT + 56.0;
        let done_w = 130.0;
        let done_h = 42.0;
        self.draw_box(
            state,
            left as i32,
            button_y as i32,
            done_w as i32,
            done_h as i32,
            (36, 92, 54, 255),
        );
        self.draw_text_line(state, "Done", left + 38.0, button_y + 10.0, 20.0, (240, 250, 240));
        self.login_done_button_bounds = (left, button_y, left + done_w, button_y + done_h);

        if is_logged_in() {
            let cancel_x = left + done_w + 14.0;
            let cancel_w = 130.0;
            self.draw_box(
                state,
                cancel_x as i32,
                button_y as i32,
                cancel_w as i32,
                done_h as i32,
                (78, 48, 48, 255),
            );
            self.draw_text_line(state, "Cancel", cancel_x + 30.0, button_y + 10.0, 20.0, (250, 232, 232));
            self.login_cancel_button_bounds = (cancel_x, button_y, cancel_x + cancel_w, button_y + done_h);
        } else {
            self.login_cancel_button_bounds = (0.0, 0.0, 0.0, 0.0);
        }

        if !self.error_message.is_empty() {
            self.draw_text_line(
                state,
                &self.error_message,
                left,
                button_y + done_h + 14.0,
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
                let incoming_rank = packet
                    .body
                    .get("from_rank")
                    .and_then(|v| v.as_u64())
                    .map(|r| r as u32)
                    .unwrap_or(packet.from_rank);
                let incoming_cursor = packet
                    .body
                    .get("cursor")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(0);
                let incoming_sel_start = packet
                    .body
                    .get("selection_start")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let incoming_sel_end = packet
                    .body
                    .get("selection_end")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                if rev > self.doc_rev || (rev == self.doc_rev && incoming_rank >= self.last_writer_rank) {
                    self.doc_rev = rev;
                    self.last_writer_rank = incoming_rank;
                    self.text_app.text_rasterizer.text = text.to_string();
                    self.text_app.apply_shared_selection_state(
                        incoming_cursor,
                        incoming_sel_start,
                        incoming_sel_end,
                    );
                    self.last_sent_text = self.text_app.text_rasterizer.text.clone();
                    let (cursor, sel_start, sel_end) = self.text_app.shared_selection_state();
                    self.last_sent_cursor = cursor;
                    self.last_sent_selection_start = sel_start;
                    self.last_sent_selection_end = sel_end;
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
        let (cursor, sel_start, sel_end) = self.text_app.shared_selection_state();
        let text_changed = current != self.last_sent_text;
        let selection_changed = cursor != self.last_sent_cursor
            || sel_start != self.last_sent_selection_start
            || sel_end != self.last_sent_selection_end;

        let nodes_now = session.current_num_nodes();
        let node_count_changed = nodes_now != self.observed_nodes;
        if node_count_changed {
            self.observed_nodes = nodes_now;
        }
        let is_host = session.rank() == 0;
        let host_anti_entropy_due = is_host
            && now.duration_since(self.last_host_anti_entropy_at) >= HOST_ANTI_ENTROPY_INTERVAL;
        let force_sync = (is_host && node_count_changed) || host_anti_entropy_due;

        if !text_changed && !selection_changed && !force_sync {
            return;
        }
        if text_changed || selection_changed {
            self.doc_rev = self.doc_rev.saturating_add(1);
            self.last_writer_rank = session.rank();
        }
        let payload = serde_json::json!({
            "rev": self.doc_rev,
            "text": current,
            "cursor": cursor,
            "selection_start": sel_start,
            "selection_end": sel_end,
            "from_rank": session.rank(),
        });
        if session.broadcast_json(DOC_KIND, payload).is_ok() {
            self.last_broadcast_at = now;
            self.last_sent_text = self.text_app.text_rasterizer.text.clone();
            self.last_sent_cursor = cursor;
            self.last_sent_selection_start = sel_start;
            self.last_sent_selection_end = sel_end;
            if is_host {
                self.last_host_anti_entropy_at = now;
            }
        }
    }

    fn username_button_hit(&self, x: f32, y: f32) -> bool {
        let (x0, y0, x1, y1) = self.username_button_bounds;
        x >= x0 && x <= x1 && y >= y0 && y <= y1
    }

    fn done_button_hit(&self, x: f32, y: f32) -> bool {
        let (x0, y0, x1, y1) = self.login_done_button_bounds;
        x >= x0 && x <= x1 && y >= y0 && y <= y1
    }

    fn cancel_button_hit(&self, x: f32, y: f32) -> bool {
        let (x0, y0, x1, y1) = self.login_cancel_button_bounds;
        x >= x0 && x <= x1 && y >= y0 && y <= y1
    }

    fn entering_login_mode(&mut self, state: &mut EngineState) {
        self.mesh_session = None;
        self.phase = AppPhase::Login;
        self.password.clear();
        self.error_message.clear();
        self.status_user_label = Self::current_username().unwrap_or_else(|| "not logged in".to_string());
        self.username = Self::current_username().unwrap_or_default();
        state.keyboard.onscreen.show();
    }

    fn process_login_keyboard_input(&mut self, state: &mut EngineState) {
        while let Some(ch) = state.keyboard.onscreen.pop_pending_char() {
            self.on_key_char(state, ch);
        }
    }

    fn submit_login(&mut self, state: &mut EngineState) {
        self.error_message.clear();
        match self.connect_mesh() {
            Ok(()) => state.keyboard.onscreen.hide(),
            Err(e) => {
                self.error_message = e;
                self.phase = AppPhase::Error;
                state.keyboard.onscreen.show();
            }
        }
    }
}

impl Application for TextMeshApp {
    fn setup(&mut self, state: &mut EngineState) -> Result<(), String> {
        self.text_app.setup(state)?;
        if self.phase == AppPhase::Editor {
            state.keyboard.onscreen.hide();
            if let Err(e) = self.connect_mesh() {
                self.error_message = e;
                self.phase = AppPhase::Login;
                state.keyboard.onscreen.show();
            }
        } else {
            state.keyboard.onscreen.show();
        }
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        match self.phase {
            AppPhase::Login | AppPhase::Error => {
                self.process_login_keyboard_input(state);
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
            self.entering_login_mode(state);
            return;
        }
        if self.phase == AppPhase::Editor {
            self.text_app.on_mouse_down(state);
            return;
        }
        if self.done_button_hit(state.mouse.x, state.mouse.y) {
            self.submit_login(state);
            return;
        }
        if is_logged_in() && self.cancel_button_hit(state.mouse.x, state.mouse.y) {
            self.phase = AppPhase::Editor;
            self.error_message.clear();
            state.keyboard.onscreen.hide();
            if self.mesh_session.is_none() {
                if let Err(e) = self.connect_mesh() {
                    self.phase = AppPhase::Error;
                    self.error_message = e;
                    state.keyboard.onscreen.show();
                }
            }
            return;
        }
        let shape = state.frame.shape();
        let width = shape[1] as f32;
        let height = shape[0] as f32;
        if state
            .keyboard
            .onscreen
            .on_mouse_down(state.mouse.x, state.mouse.y, width, height)
        {
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
        } else {
            state.keyboard.onscreen.on_mouse_up();
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
                self.submit_login(state);
            }
            '\u{1b}' => {
                self.error_message.clear();
                self.phase = AppPhase::Login;
                state.keyboard.onscreen.show();
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
