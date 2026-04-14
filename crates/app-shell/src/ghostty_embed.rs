use eframe::egui::{self, Rect};

use crate::ghostty::WorkspaceRequest;

#[derive(Clone, Debug)]
pub struct EmbeddedGhosttySnapshot {
    pub backend_label: &'static str,
    pub version: Option<String>,
    pub message: String,
}

pub struct EmbeddedGhostty {
    backend: Backend,
}

impl EmbeddedGhostty {
    pub fn new(cc: &eframe::CreationContext<'_>, request: &WorkspaceRequest) -> Self {
        Self {
            backend: Backend::new(cc, request),
        }
    }

    pub fn available(&self) -> bool {
        self.backend.available()
    }

    pub fn snapshot(&self) -> EmbeddedGhosttySnapshot {
        self.backend.snapshot()
    }

    pub fn sync(
        &mut self,
        frame: &eframe::Frame,
        rect: Rect,
        visible: bool,
        request: &WorkspaceRequest,
    ) {
        self.backend.sync(frame, rect, visible, request);
    }
}

#[cfg(all(target_os = "macos", ghostty_embed_available))]
mod platform {
    use std::{
        env,
        ffi::{CStr, CString, c_char, c_void},
        path::{Path, PathBuf},
        ptr,
        sync::{
            Arc, OnceLock,
            atomic::{AtomicBool, AtomicPtr, Ordering},
            mpsc::{self, Receiver, Sender},
        },
    };

    use eframe::egui::Rect;
    use objc2::{
        ClassType, DeclaredClass, declare_class, msg_send_id, mutability,
        rc::{Retained, autoreleasepool_leaking},
    };
    use objc2_app_kit::{
        NSEvent, NSEventModifierFlags, NSPasteboard, NSPasteboardTypeString, NSResponder, NSView,
    };
    use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use crate::ghostty::WorkspaceRequest;

    use super::EmbeddedGhosttySnapshot;

    #[allow(
        non_camel_case_types,
        non_snake_case,
        non_upper_case_globals,
        dead_code,
        improper_ctypes,
        clippy::all
    )]
    mod bindings {
        include!(concat!(env!("OUT_DIR"), "/ghostty_bindings.rs"));
    }

    #[derive(Debug)]
    struct HostViewIvars {
        controller: *const HostController,
    }

    declare_class!(
        struct GhosttyHostView;

        unsafe impl ClassType for GhosttyHostView {
            #[inherits(NSResponder)]
            type Super = NSView;
            type Mutability = mutability::MainThreadOnly;
            const NAME: &'static str = "GhosttyShellEmbeddedHostView";
        }

        impl DeclaredClass for GhosttyHostView {
            type Ivars = HostViewIvars;
        }

        unsafe impl GhosttyHostView {
            #[method(isFlipped)]
            fn is_flipped(&self) -> bool {
                true
            }

            #[method(acceptsFirstResponder)]
            fn accepts_first_responder(&self) -> bool {
                true
            }

            #[method(acceptsFirstMouse:)]
            fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
                true
            }

            #[method(viewDidMoveToWindow)]
            fn view_did_move_to_window(&self) {
                if let Some(window) = self.window() {
                    window.setAcceptsMouseMovedEvents(true);
                }
            }

            #[method(becomeFirstResponder)]
            fn become_first_responder(&self) -> bool {
                self.controller().set_focus(true);
                true
            }

            #[method(resignFirstResponder)]
            fn resign_first_responder(&self) -> bool {
                self.controller().set_focus(false);
                true
            }

            #[method(mouseDown:)]
            fn mouse_down(&self, event: &NSEvent) {
                self.focus_window();
                self.controller().send_mouse_pos(self, event);
                self.controller()
                    .send_mouse_button(
                        event,
                        bindings::ghostty_input_mouse_state_e_GHOSTTY_MOUSE_PRESS,
                        mouse_button(0),
                    );
            }

            #[method(mouseUp:)]
            fn mouse_up(&self, event: &NSEvent) {
                self.controller()
                    .send_mouse_button(
                        event,
                        bindings::ghostty_input_mouse_state_e_GHOSTTY_MOUSE_RELEASE,
                        mouse_button(0),
                    );
            }

            #[method(rightMouseDown:)]
            fn right_mouse_down(&self, event: &NSEvent) {
                self.focus_window();
                self.controller().send_mouse_pos(self, event);
                self.controller()
                    .send_mouse_button(
                        event,
                        bindings::ghostty_input_mouse_state_e_GHOSTTY_MOUSE_PRESS,
                        mouse_button(1),
                    );
            }

            #[method(rightMouseUp:)]
            fn right_mouse_up(&self, event: &NSEvent) {
                self.controller()
                    .send_mouse_button(
                        event,
                        bindings::ghostty_input_mouse_state_e_GHOSTTY_MOUSE_RELEASE,
                        mouse_button(1),
                    );
            }

            #[method(otherMouseDown:)]
            fn other_mouse_down(&self, event: &NSEvent) {
                self.focus_window();
                self.controller().send_mouse_pos(self, event);
                self.controller().send_mouse_button(
                    event,
                    bindings::ghostty_input_mouse_state_e_GHOSTTY_MOUSE_PRESS,
                    mouse_button(unsafe { event.buttonNumber() } as usize),
                );
            }

            #[method(otherMouseUp:)]
            fn other_mouse_up(&self, event: &NSEvent) {
                self.controller().send_mouse_button(
                    event,
                    bindings::ghostty_input_mouse_state_e_GHOSTTY_MOUSE_RELEASE,
                    mouse_button(unsafe { event.buttonNumber() } as usize),
                );
            }

            #[method(mouseMoved:)]
            fn mouse_moved(&self, event: &NSEvent) {
                self.controller().send_mouse_pos(self, event);
            }

            #[method(mouseDragged:)]
            fn mouse_dragged(&self, event: &NSEvent) {
                self.controller().send_mouse_pos(self, event);
            }

            #[method(rightMouseDragged:)]
            fn right_mouse_dragged(&self, event: &NSEvent) {
                self.controller().send_mouse_pos(self, event);
            }

            #[method(otherMouseDragged:)]
            fn other_mouse_dragged(&self, event: &NSEvent) {
                self.controller().send_mouse_pos(self, event);
            }

            #[method(scrollWheel:)]
            fn scroll_wheel(&self, event: &NSEvent) {
                self.controller().send_scroll(event);
            }

            #[method(keyDown:)]
            fn key_down(&self, event: &NSEvent) {
                self.controller().send_key_event(event, key_action(event, true));
            }

            #[method(keyUp:)]
            fn key_up(&self, event: &NSEvent) {
                self.controller()
                    .send_key_event(
                        event,
                        bindings::ghostty_input_action_e_GHOSTTY_ACTION_RELEASE,
                    );
            }
        }
    );

    impl GhosttyHostView {
        fn new(mtm: MainThreadMarker, controller: *const HostController) -> Retained<Self> {
            let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1.0, 1.0));
            let this = mtm.alloc();
            let this = this.set_ivars(HostViewIvars { controller });
            unsafe { msg_send_id![super(this), initWithFrame: frame] }
        }

        fn controller(&self) -> &HostController {
            let controller = self.ivars().controller;
            debug_assert!(!controller.is_null());
            unsafe { &*controller }
        }

        fn focus_window(&self) {
            if let Some(window) = self.window() {
                let responder: &NSResponder = self;
                let _ = window.makeFirstResponder(Some(responder));
            }
        }
    }

    #[derive(Debug)]
    struct HostController {
        surface: AtomicPtr<c_void>,
        app: AtomicPtr<c_void>,
        runtime_tx: Sender<RuntimeEvent>,
        closing: AtomicBool,
    }

    impl HostController {
        fn set_surface(&self, surface: bindings::ghostty_surface_t) {
            self.surface.store(surface.cast(), Ordering::SeqCst);
        }

        fn set_app(&self, app: bindings::ghostty_app_t) {
            self.app.store(app.cast(), Ordering::SeqCst);
        }

        fn surface(&self) -> Option<bindings::ghostty_surface_t> {
            let surface = self.surface.load(Ordering::SeqCst);
            if surface.is_null() {
                None
            } else {
                Some(surface.cast())
            }
        }

        fn app(&self) -> Option<bindings::ghostty_app_t> {
            let app = self.app.load(Ordering::SeqCst);
            if app.is_null() {
                None
            } else {
                Some(app.cast())
            }
        }

        fn set_focus(&self, focused: bool) {
            if self.closing.load(Ordering::SeqCst) {
                return;
            }

            if let Some(surface) = self.surface() {
                unsafe {
                    bindings::ghostty_surface_set_focus(surface, focused);
                }
            }

            if let Some(app) = self.app() {
                unsafe {
                    bindings::ghostty_app_set_focus(app, focused);
                }
            }
        }

        fn send_key_event(&self, event: &NSEvent, action: bindings::ghostty_input_action_e) {
            let Some(surface) = self.surface() else {
                return;
            };

            let mods = ghostty_mods(unsafe { event.modifierFlags() });
            let consumed_mods =
                unsafe { bindings::ghostty_surface_key_translation_mods(surface, mods) };
            let text = ghostty_characters(event);
            let unshifted_codepoint = ghostty_unshifted_codepoint(event);
            let mut key_event = bindings::ghostty_input_key_s {
                action,
                mods,
                consumed_mods,
                keycode: unsafe { event.keyCode() } as u32,
                text: ptr::null(),
                unshifted_codepoint,
                composing: false,
            };

            match text {
                Some(text) => {
                    let text = CString::new(text).unwrap_or_default();
                    key_event.text = text.as_ptr();
                    unsafe {
                        bindings::ghostty_surface_key(surface, key_event);
                    }
                }
                None => unsafe {
                    bindings::ghostty_surface_key(surface, key_event);
                },
            }
        }

        fn send_mouse_button(
            &self,
            event: &NSEvent,
            state: bindings::ghostty_input_mouse_state_e,
            button: bindings::ghostty_input_mouse_button_e,
        ) {
            let Some(surface) = self.surface() else {
                return;
            };

            unsafe {
                bindings::ghostty_surface_mouse_button(
                    surface,
                    state,
                    button,
                    ghostty_mods(event.modifierFlags()),
                );
            }
        }

        fn send_mouse_pos(&self, view: &NSView, event: &NSEvent) {
            let Some(surface) = self.surface() else {
                return;
            };

            let point = unsafe { view.convertPoint_fromView(event.locationInWindow(), None) };
            unsafe {
                bindings::ghostty_surface_mouse_pos(
                    surface,
                    point.x,
                    point.y,
                    ghostty_mods(event.modifierFlags()),
                );
            }
        }

        fn send_scroll(&self, event: &NSEvent) {
            let Some(surface) = self.surface() else {
                return;
            };

            let mut mods: i32 = 0;
            if unsafe { event.hasPreciseScrollingDeltas() } {
                mods |= 0b1;
            }

            unsafe {
                bindings::ghostty_surface_mouse_scroll(
                    surface,
                    event.scrollingDeltaX(),
                    event.scrollingDeltaY(),
                    mods,
                );
            }
        }
    }

    #[derive(Debug)]
    enum RuntimeEvent {
        Wakeup,
        SurfaceClosed(bool),
    }

    pub(super) struct Backend {
        root_view: Retained<NSView>,
        host_view: Retained<GhosttyHostView>,
        controller: Arc<HostController>,
        runtime_rx: Receiver<RuntimeEvent>,
        app: bindings::ghostty_app_t,
        config: bindings::ghostty_config_t,
        surface: bindings::ghostty_surface_t,
        current_directory: PathBuf,
        version: Option<String>,
        message: String,
        visible: bool,
    }

    impl Backend {
        pub(super) fn new(
            cc: &eframe::CreationContext<'_>,
            request: &WorkspaceRequest,
        ) -> Result<Self, String> {
            init_ghostty_once()?;

            let root_view = extract_root_view(cc)?;
            let (runtime_tx, runtime_rx) = mpsc::channel();
            let controller = Arc::new(HostController {
                surface: AtomicPtr::new(ptr::null_mut()),
                app: AtomicPtr::new(ptr::null_mut()),
                runtime_tx,
                closing: AtomicBool::new(false),
            });

            let mtm =
                MainThreadMarker::new().ok_or("Ghostty embedding requires the main thread")?;
            let host_view = GhosttyHostView::new(mtm, Arc::as_ptr(&controller));

            unsafe {
                root_view.addSubview(&host_view);
            }
            host_view.setHidden(true);

            let config = unsafe { bindings::ghostty_config_new() };
            if config.is_null() {
                return Err("ghostty_config_new returned null".into());
            }

            unsafe {
                bindings::ghostty_config_load_default_files(config);
                bindings::ghostty_config_load_recursive_files(config);
                bindings::ghostty_config_finalize(config);
            }

            let mut runtime_config = bindings::ghostty_runtime_config_s {
                userdata: Arc::as_ptr(&controller).cast_mut().cast(),
                supports_selection_clipboard: true,
                wakeup_cb: Some(runtime_wakeup),
                action_cb: Some(runtime_action),
                read_clipboard_cb: Some(runtime_read_clipboard),
                confirm_read_clipboard_cb: Some(runtime_confirm_read_clipboard),
                write_clipboard_cb: Some(runtime_write_clipboard),
                close_surface_cb: Some(runtime_close_surface),
            };

            let app = unsafe { bindings::ghostty_app_new(&mut runtime_config, config) };
            if app.is_null() {
                unsafe {
                    bindings::ghostty_config_free(config);
                }
                return Err("ghostty_app_new returned null".into());
            }

            controller.set_app(app);

            let surface = create_surface(
                app,
                &host_view,
                &controller,
                request.working_directory.as_path(),
            )?;
            controller.set_surface(surface);

            let version = ghostty_version();
            let message = format!(
                "Embedded Ghostty is live inside the shell at {}.",
                request.working_directory.display()
            );

            Ok(Self {
                root_view,
                host_view,
                controller,
                runtime_rx,
                app,
                config,
                surface,
                current_directory: request.working_directory.clone(),
                version,
                message,
                visible: false,
            })
        }

        pub(super) fn available(&self) -> bool {
            !self.surface.is_null()
        }

        pub(super) fn snapshot(&self) -> EmbeddedGhosttySnapshot {
            EmbeddedGhosttySnapshot {
                backend_label: "Embedded libghostty",
                version: self.version.clone(),
                message: self.message.clone(),
            }
        }

        pub(super) fn sync(
            &mut self,
            _frame: &eframe::Frame,
            rect: Rect,
            visible: bool,
            request: &WorkspaceRequest,
        ) {
            self.drain_runtime_events();

            if self.current_directory != request.working_directory {
                if let Err(error) = self.recreate_surface(request.working_directory.as_path()) {
                    self.message = error;
                }
            }

            self.host_view.setHidden(!visible);
            self.controller.set_focus(visible);

            if let Some(surface) = self.controller.surface() {
                unsafe {
                    bindings::ghostty_surface_set_occlusion(surface, !visible);
                }
            }

            self.visible = visible;
            if !visible {
                return;
            }

            let bounds = self.root_view.bounds();
            let frame = NSRect::new(
                NSPoint::new(rect.min.x as f64, bounds.size.height - rect.max.y as f64),
                NSSize::new(rect.width() as f64, rect.height() as f64),
            );
            unsafe {
                self.host_view.setFrame(frame);
            }

            if let Some(surface) = self.controller.surface() {
                let backing = unsafe { self.root_view.convertRectToBacking(frame) };
                let width = backing.size.width.max(1.0).round() as u32;
                let height = backing.size.height.max(1.0).round() as u32;
                let scale_x = if frame.size.width > 0.0 {
                    backing.size.width / frame.size.width
                } else {
                    1.0
                };
                let scale_y = if frame.size.height > 0.0 {
                    backing.size.height / frame.size.height
                } else {
                    1.0
                };

                unsafe {
                    bindings::ghostty_surface_set_content_scale(surface, scale_x, scale_y);
                    bindings::ghostty_surface_set_size(surface, width, height);
                }
            }
        }

        fn drain_runtime_events(&mut self) {
            while let Ok(event) = self.runtime_rx.try_recv() {
                match event {
                    RuntimeEvent::Wakeup => unsafe {
                        bindings::ghostty_app_tick(self.app);
                    },
                    RuntimeEvent::SurfaceClosed(process_alive) => {
                        self.message = if process_alive {
                            "Embedded Ghostty requested a surface close while the child is still alive."
                                .into()
                        } else {
                            "Embedded Ghostty surface exited. Rebuilding it on the next frame."
                                .into()
                        };
                        let directory = self.current_directory.clone();
                        let _ = self.recreate_surface(directory.as_path());
                    }
                }
            }
        }

        fn recreate_surface(&mut self, working_directory: &Path) -> Result<(), String> {
            if let Some(surface) = self.controller.surface() {
                unsafe {
                    bindings::ghostty_surface_free(surface);
                }
                self.controller.set_surface(ptr::null_mut());
            }

            let surface = create_surface(
                self.app,
                &self.host_view,
                &self.controller,
                working_directory,
            )?;
            self.controller.set_surface(surface);
            self.surface = surface;
            self.current_directory = working_directory.to_path_buf();
            self.message = format!(
                "Embedded Ghostty is live inside the shell at {}.",
                self.current_directory.display()
            );
            Ok(())
        }
    }

    impl Drop for Backend {
        fn drop(&mut self) {
            self.controller.closing.store(true, Ordering::SeqCst);
            self.host_view.setHidden(true);
            unsafe {
                self.host_view.removeFromSuperview();
            }

            if let Some(surface) = self.controller.surface() {
                unsafe {
                    bindings::ghostty_surface_free(surface);
                }
                self.controller.set_surface(ptr::null_mut());
            }

            unsafe {
                bindings::ghostty_app_free(self.app);
                bindings::ghostty_config_free(self.config);
            }
        }
    }

    fn init_ghostty_once() -> Result<(), String> {
        static INIT_RESULT: OnceLock<Result<(), String>> = OnceLock::new();

        INIT_RESULT
            .get_or_init(|| {
                let mut args = env::args()
                    .map(|arg| CString::new(arg).map_err(|_| "invalid CLI argument".to_string()))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut argv = args
                    .iter_mut()
                    .map(|arg| arg.as_ptr().cast_mut())
                    .collect::<Vec<*mut c_char>>();
                let result =
                    unsafe { bindings::ghostty_init(argv.len() as usize, argv.as_mut_ptr()) };

                if result == bindings::GHOSTTY_SUCCESS as i32 {
                    Ok(())
                } else {
                    Err(format!("ghostty_init failed with status {result}"))
                }
            })
            .clone()
    }

    fn extract_root_view(cc: &eframe::CreationContext<'_>) -> Result<Retained<NSView>, String> {
        let handle = cc.window_handle().map_err(|error| error.to_string())?;
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return Err("eframe did not expose an AppKit window handle".into());
        };

        let ns_view_ptr = appkit.ns_view.as_ptr().cast::<NSView>();
        if ns_view_ptr.is_null() {
            return Err("eframe AppKit view handle was null".into());
        }

        unsafe {
            Retained::retain(ns_view_ptr).ok_or_else(|| "failed to retain root NSView".into())
        }
    }

    fn create_surface(
        app: bindings::ghostty_app_t,
        host_view: &GhosttyHostView,
        controller: &Arc<HostController>,
        working_directory: &Path,
    ) -> Result<bindings::ghostty_surface_t, String> {
        let working_directory = CString::new(working_directory.to_string_lossy().into_owned())
            .map_err(|_| "working directory contains interior nul".to_string())?;

        let bounds = host_view.bounds();
        let backing = unsafe { host_view.convertRectToBacking(bounds) };
        let width = backing.size.width.max(1.0).round() as u32;
        let height = backing.size.height.max(1.0).round() as u32;
        let scale_factor = if bounds.size.width > 0.0 {
            backing.size.width / bounds.size.width
        } else {
            1.0
        };

        let mut surface_config = unsafe { bindings::ghostty_surface_config_new() };
        surface_config.platform_tag = bindings::ghostty_platform_e_GHOSTTY_PLATFORM_MACOS;
        surface_config.platform.macos.nsview = host_view as *const GhosttyHostView as *mut c_void;
        surface_config.userdata = Arc::as_ptr(controller).cast_mut().cast();
        surface_config.scale_factor = scale_factor;
        surface_config.working_directory = working_directory.as_ptr();
        surface_config.context = bindings::ghostty_surface_context_e_GHOSTTY_SURFACE_CONTEXT_WINDOW;

        let surface = unsafe { bindings::ghostty_surface_new(app, &mut surface_config) };
        if surface.is_null() {
            return Err(format!(
                "ghostty_surface_new failed for {}",
                working_directory.to_string_lossy()
            ));
        }

        unsafe {
            bindings::ghostty_surface_set_content_scale(surface, scale_factor, scale_factor);
            bindings::ghostty_surface_set_size(surface, width, height);
            bindings::ghostty_surface_set_focus(surface, true);
            bindings::ghostty_surface_set_occlusion(surface, false);
        }

        Ok(surface)
    }

    fn ghostty_version() -> Option<String> {
        let info = unsafe { bindings::ghostty_info() };
        if info.version.is_null() || info.version_len == 0 {
            return None;
        }

        let bytes =
            unsafe { std::slice::from_raw_parts(info.version.cast::<u8>(), info.version_len) };
        Some(String::from_utf8_lossy(bytes).into_owned())
    }

    fn ghostty_mods(flags: NSEventModifierFlags) -> bindings::ghostty_input_mods_e {
        let mut mods = bindings::ghostty_input_mods_e_GHOSTTY_MODS_NONE;
        if flags.contains(NSEventModifierFlags::NSEventModifierFlagShift) {
            mods |= bindings::ghostty_input_mods_e_GHOSTTY_MODS_SHIFT;
        }
        if flags.contains(NSEventModifierFlags::NSEventModifierFlagControl) {
            mods |= bindings::ghostty_input_mods_e_GHOSTTY_MODS_CTRL;
        }
        if flags.contains(NSEventModifierFlags::NSEventModifierFlagOption) {
            mods |= bindings::ghostty_input_mods_e_GHOSTTY_MODS_ALT;
        }
        if flags.contains(NSEventModifierFlags::NSEventModifierFlagCommand) {
            mods |= bindings::ghostty_input_mods_e_GHOSTTY_MODS_SUPER;
        }
        if flags.contains(NSEventModifierFlags::NSEventModifierFlagCapsLock) {
            mods |= bindings::ghostty_input_mods_e_GHOSTTY_MODS_CAPS;
        }
        mods
    }

    fn ghostty_characters(event: &NSEvent) -> Option<String> {
        let characters = unsafe { event.characters() }?;
        let text = nsstring_to_string(&characters);
        let mut scalars = text.chars();
        let Some(first) = scalars.next() else {
            return None;
        };

        if scalars.next().is_none() {
            let codepoint = first as u32;
            if codepoint < 0x20 || (0xF700..=0xF8FF).contains(&codepoint) {
                return None;
            }
        }

        Some(text)
    }

    fn ghostty_unshifted_codepoint(event: &NSEvent) -> u32 {
        unsafe { event.charactersByApplyingModifiers(NSEventModifierFlags::empty()) }
            .map(|characters| nsstring_to_string(&characters))
            .and_then(|text| text.chars().next())
            .map(|character| character as u32)
            .unwrap_or(0)
    }

    fn nsstring_to_string(value: &NSString) -> String {
        autoreleasepool_leaking(|pool| value.as_str(pool).to_owned())
    }

    fn key_action(event: &NSEvent, down: bool) -> bindings::ghostty_input_action_e {
        if down && unsafe { event.isARepeat() } {
            bindings::ghostty_input_action_e_GHOSTTY_ACTION_REPEAT
        } else if down {
            bindings::ghostty_input_action_e_GHOSTTY_ACTION_PRESS
        } else {
            bindings::ghostty_input_action_e_GHOSTTY_ACTION_RELEASE
        }
    }

    fn mouse_button(button_number: usize) -> bindings::ghostty_input_mouse_button_e {
        match button_number {
            0 => bindings::ghostty_input_mouse_button_e_GHOSTTY_MOUSE_LEFT,
            1 => bindings::ghostty_input_mouse_button_e_GHOSTTY_MOUSE_RIGHT,
            2 => bindings::ghostty_input_mouse_button_e_GHOSTTY_MOUSE_MIDDLE,
            3 => bindings::ghostty_input_mouse_button_e_GHOSTTY_MOUSE_EIGHT,
            4 => bindings::ghostty_input_mouse_button_e_GHOSTTY_MOUSE_NINE,
            5 => bindings::ghostty_input_mouse_button_e_GHOSTTY_MOUSE_SIX,
            6 => bindings::ghostty_input_mouse_button_e_GHOSTTY_MOUSE_SEVEN,
            7 => bindings::ghostty_input_mouse_button_e_GHOSTTY_MOUSE_FOUR,
            8 => bindings::ghostty_input_mouse_button_e_GHOSTTY_MOUSE_FIVE,
            9 => bindings::ghostty_input_mouse_button_e_GHOSTTY_MOUSE_TEN,
            10 => bindings::ghostty_input_mouse_button_e_GHOSTTY_MOUSE_ELEVEN,
            _ => bindings::ghostty_input_mouse_button_e_GHOSTTY_MOUSE_UNKNOWN,
        }
    }

    unsafe extern "C" fn runtime_wakeup(userdata: *mut c_void) {
        if userdata.is_null() {
            return;
        }

        let controller = unsafe { &*(userdata.cast::<HostController>()) };
        let _ = controller.runtime_tx.send(RuntimeEvent::Wakeup);
    }

    unsafe extern "C" fn runtime_action(
        _app: bindings::ghostty_app_t,
        _target: bindings::ghostty_target_s,
        _action: bindings::ghostty_action_s,
    ) -> bool {
        false
    }

    unsafe extern "C" fn runtime_read_clipboard(
        userdata: *mut c_void,
        _location: bindings::ghostty_clipboard_e,
        state: *mut c_void,
    ) -> bool {
        if userdata.is_null() {
            return false;
        }

        let controller = unsafe { &*(userdata.cast::<HostController>()) };
        let Some(surface) = controller.surface() else {
            return false;
        };

        let pasteboard = unsafe { NSPasteboard::generalPasteboard() };
        let Some(contents) = (unsafe { pasteboard.stringForType(NSPasteboardTypeString) }) else {
            return false;
        };
        let string = nsstring_to_string(&contents);
        let Ok(text) = CString::new(string) else {
            return false;
        };

        unsafe {
            bindings::ghostty_surface_complete_clipboard_request(
                surface,
                text.as_ptr(),
                state,
                false,
            );
        }
        true
    }

    unsafe extern "C" fn runtime_confirm_read_clipboard(
        userdata: *mut c_void,
        value: *const c_char,
        state: *mut c_void,
        _request: bindings::ghostty_clipboard_request_e,
    ) {
        if userdata.is_null() || value.is_null() {
            return;
        }

        let controller = unsafe { &*(userdata.cast::<HostController>()) };
        let Some(surface) = controller.surface() else {
            return;
        };

        unsafe {
            bindings::ghostty_surface_complete_clipboard_request(surface, value, state, true);
        }
    }

    unsafe extern "C" fn runtime_write_clipboard(
        _userdata: *mut c_void,
        _location: bindings::ghostty_clipboard_e,
        content: *const bindings::ghostty_clipboard_content_s,
        len: usize,
        _confirm: bool,
    ) {
        if content.is_null() || len == 0 {
            return;
        }

        let pasteboard = unsafe { NSPasteboard::generalPasteboard() };
        unsafe {
            pasteboard.clearContents();
        }

        for index in 0..len {
            let entry = unsafe { &*content.add(index) };
            if entry.mime.is_null() || entry.data.is_null() {
                continue;
            }

            let mime = unsafe { CStr::from_ptr(entry.mime) }.to_string_lossy();
            if mime != "text/plain" {
                continue;
            }

            let text = unsafe { CStr::from_ptr(entry.data) }
                .to_string_lossy()
                .into_owned();
            let text = NSString::from_str(&text);
            let _ = unsafe { pasteboard.setString_forType(&text, NSPasteboardTypeString) };
            break;
        }
    }

    unsafe extern "C" fn runtime_close_surface(userdata: *mut c_void, process_alive: bool) {
        if userdata.is_null() {
            return;
        }

        let controller = unsafe { &*(userdata.cast::<HostController>()) };
        let _ = controller
            .runtime_tx
            .send(RuntimeEvent::SurfaceClosed(process_alive));
    }
}

impl Backend {
    fn new(_cc: &eframe::CreationContext<'_>, _request: &WorkspaceRequest) -> Self {
        #[cfg(all(target_os = "macos", ghostty_embed_available))]
        {
            return match platform::Backend::new(_cc, _request) {
                Ok(backend) => Self::Platform(backend),
                Err(error) => Self::Stub(EmbeddedGhosttySnapshot {
                    backend_label: "Embedded libghostty",
                    version: None,
                    message: error,
                }),
            };
        }

        #[cfg(not(all(target_os = "macos", ghostty_embed_available)))]
        {
            Self::Stub(EmbeddedGhosttySnapshot {
                backend_label: "Embedded libghostty",
                version: None,
                message: "Embedded Ghostty is unavailable. Run `scripts/bootstrap_ghostty_latest.sh` to build GhosttyKit first.".into(),
            })
        }
    }

    fn available(&self) -> bool {
        match self {
            #[cfg(all(target_os = "macos", ghostty_embed_available))]
            Self::Platform(backend) => backend.available(),
            Self::Stub(_) => false,
        }
    }

    fn snapshot(&self) -> EmbeddedGhosttySnapshot {
        match self {
            #[cfg(all(target_os = "macos", ghostty_embed_available))]
            Self::Platform(backend) => backend.snapshot(),
            Self::Stub(snapshot) => snapshot.clone(),
        }
    }

    fn sync(
        &mut self,
        _frame: &eframe::Frame,
        _rect: egui::Rect,
        _visible: bool,
        _request: &WorkspaceRequest,
    ) {
        match self {
            #[cfg(all(target_os = "macos", ghostty_embed_available))]
            Self::Platform(backend) => backend.sync(_frame, _rect, _visible, _request),
            Self::Stub(_) => {}
        }
    }
}

enum Backend {
    #[cfg(all(target_os = "macos", ghostty_embed_available))]
    Platform(platform::Backend),
    Stub(EmbeddedGhosttySnapshot),
}
