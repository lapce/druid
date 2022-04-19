// Copyright 2019 The Druid Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Window building and app lifecycle.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::ext_event::{ExtEventHost, ExtEventSink};
use crate::kurbo::{Point, Size};
use crate::menu::MenuManager;
use crate::shell::{Application, Error as PlatformError, WindowBuilder, WindowHandle, WindowLevel};
use crate::widget::LabelText;
use crate::win_handler::{AppHandler, AppState};
use crate::window::WindowId;
use crate::{AppDelegate, Data, Env, Event, LocalizedString, Menu, MouseEvent, Widget};

use druid_shell::kurbo::Vec2;
use druid_shell::{
    glutin_key, KbKey, KeyEvent, KeyState, Modifiers, MouseButton, MouseButtons, TimerToken,
    WindowState, WinitEvent,
};
use glutin::event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget};

/// A function that modifies the initial environment.
type EnvSetupFn<T> = dyn FnOnce(&mut Env, &T);

/// Handles initial setup of an application, and starts the runloop.
pub struct AppLauncher<T> {
    windows: Vec<WindowDesc<T>>,
    env_setup: Option<Box<EnvSetupFn<T>>>,
    l10n_resources: Option<(Vec<String>, String)>,
    delegate: Option<Box<dyn AppDelegate<T>>>,
    ext_event_host: ExtEventHost,
}

/// Defines how a windows size should be determined
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum WindowSizePolicy {
    /// Use the content of the window to determine the size.
    ///
    /// If you use this option, your root widget will be passed infinite constraints;
    /// you are responsible for ensuring that your content picks an appropriate size.
    Content,
    /// Use the provided window size
    User,
}

/// Window configuration that can be applied to a WindowBuilder, or to an existing WindowHandle.
/// It does not include anything related to app data.
#[derive(Debug, PartialEq)]
pub struct WindowConfig {
    pub(crate) size_policy: WindowSizePolicy,
    pub(crate) size: Option<Size>,
    pub(crate) min_size: Option<Size>,
    pub(crate) position: Option<Point>,
    pub(crate) resizable: Option<bool>,
    pub(crate) transparent: Option<bool>,
    pub(crate) show_titlebar: Option<bool>,
    pub(crate) set_title: Option<String>,
    pub(crate) level: Option<WindowLevel>,
    pub(crate) state: Option<WindowState>,
}

/// A description of a window to be instantiated.
pub struct WindowDesc<T> {
    pub(crate) pending: PendingWindow<T>,
    pub(crate) config: WindowConfig,
    /// The `WindowId` that will be assigned to this window.
    ///
    /// This can be used to track a window from when it is launched and when
    /// it actually connects.
    pub id: WindowId,
}

/// The parts of a window, pending construction, that are dependent on top level app state
/// or are not part of the druid shells windowing abstraction.
/// This includes the boxed root widget, as well as other window properties such as the title.
pub struct PendingWindow<T> {
    pub(crate) root: Box<dyn Widget<T>>,
    pub(crate) title: LabelText<T>,
    pub(crate) transparent: bool,
    pub(crate) menu: Option<MenuManager<T>>,
    pub(crate) size_policy: WindowSizePolicy, // This is copied over from the WindowConfig
                                              // when the native window is constructed.
}

impl<T: Data> PendingWindow<T> {
    /// Create a pending window from any widget.
    pub fn new<W>(root: W) -> PendingWindow<T>
    where
        W: Widget<T> + 'static,
    {
        // This just makes our API slightly cleaner; callers don't need to explicitly box.
        PendingWindow {
            root: Box::new(root),
            title: LocalizedString::new("app-name").into(),
            menu: MenuManager::platform_default(),
            transparent: false,
            size_policy: WindowSizePolicy::User,
        }
    }

    /// Set the title for this window. This is a [`LabelText`]; it can be either
    /// a `String`, a [`LocalizedString`], or a closure that computes a string;
    /// it will be kept up to date as the application's state changes.
    ///
    /// [`LabelText`]: widget/enum.LocalizedString.html
    /// [`LocalizedString`]: struct.LocalizedString.html
    pub fn title(mut self, title: impl Into<LabelText<T>>) -> Self {
        self.title = title.into();
        self
    }

    /// Set wether the background should be transparent
    pub fn transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    /// Set the menu for this window.
    ///
    /// `menu` is a callback for creating the menu. Its first argument is the id of the window that
    /// will have the menu, or `None` if it's creating the root application menu for an app with no
    /// menus (which can happen, for example, on macOS).
    pub fn menu(
        mut self,
        menu: impl FnMut(Option<WindowId>, &T, &Env) -> Menu<T> + 'static,
    ) -> Self {
        self.menu = Some(MenuManager::new(menu));
        self
    }
}

impl<T: Data> AppLauncher<T> {
    pub fn new() -> Self {
        AppLauncher {
            windows: vec![],
            env_setup: None,
            l10n_resources: None,
            delegate: None,
            ext_event_host: ExtEventHost::new(),
        }
    }

    /// Create a new `AppLauncher` with the provided window.
    pub fn with_window(mut self, window: WindowDesc<T>) -> Self {
        self.windows.push(window);
        self
    }

    /// Provide an optional closure that will be given mutable access to
    /// the environment and immutable access to the app state before launch.
    ///
    /// This can be used to set or override theme values.
    pub fn configure_env(mut self, f: impl Fn(&mut Env, &T) + 'static) -> Self {
        self.env_setup = Some(Box::new(f));
        self
    }

    /// Set the [`AppDelegate`].
    ///
    /// [`AppDelegate`]: trait.AppDelegate.html
    pub fn delegate(mut self, delegate: impl AppDelegate<T> + 'static) -> Self {
        self.delegate = Some(Box::new(delegate));
        self
    }

    /// Initialize a minimal logger with DEBUG max level for printing logs out to stderr.
    ///
    /// This is meant for use during development only.
    ///
    /// # Panics
    ///
    /// Panics if the logger fails to initialize.
    #[deprecated(since = "0.7.0", note = "Use log_to_console instead")]
    pub fn use_simple_logger(self) -> Self {
        self.log_to_console()
    }

    /// Initialize a minimal tracing subscriber with DEBUG max level for printing logs out to
    /// stderr.
    ///
    /// This is meant for quick-and-dirty debugging. If you want more serious trace handling,
    /// it's probably better to implement it yourself.
    ///
    /// # Panics
    ///
    /// Panics if the subscriber fails to initialize.
    pub fn log_to_console(self) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use tracing_subscriber::prelude::*;
            let filter_layer = tracing_subscriber::filter::LevelFilter::DEBUG;
            let fmt_layer = tracing_subscriber::fmt::layer()
                // Display target (eg "my_crate::some_mod::submod") with logs
                .with_target(true);

            tracing_subscriber::registry()
                .with(filter_layer)
                .with(fmt_layer)
                .init();
        }
        // Note - tracing-wasm might not work in headless Node.js. Probably doesn't matter anyway,
        // because this is a GUI framework, so wasm targets will virtually always be browsers.
        #[cfg(target_arch = "wasm32")]
        {
            console_error_panic_hook::set_once();
            let config = tracing_wasm::WASMLayerConfigBuilder::new()
                .set_max_level(tracing::Level::DEBUG)
                .build();
            tracing_wasm::set_as_global_default_with_config(config)
        }
        self
    }

    /// Use custom localization resource
    ///
    /// `resources` is a list of file names that contain strings. `base_dir`
    /// is a path to a directory that includes per-locale subdirectories.
    ///
    /// This directory should be of the structure `base_dir/{locale}/{resource}`,
    /// where '{locale}' is a valid BCP47 language tag, and {resource} is a `.ftl`
    /// included in `resources`.
    pub fn localization_resources(mut self, resources: Vec<String>, base_dir: String) -> Self {
        self.l10n_resources = Some((resources, base_dir));
        self
    }

    /// Returns an [`ExtEventSink`] that can be moved between threads,
    /// and can be used to submit commands back to the application.
    ///
    /// [`ExtEventSink`]: struct.ExtEventSink.html
    pub fn get_external_handle(&self) -> ExtEventSink {
        self.ext_event_host.make_sink()
    }

    /// Build the windows and start the runloop.
    ///
    /// Returns an error if a window cannot be instantiated. This is usually
    /// a fatal error.
    pub fn launch(mut self, data: T) -> Result<(), PlatformError> {
        let event_loop = EventLoop::with_user_event();
        let event_proxy = Arc::new(event_loop.create_proxy());

        let app = Application::new(event_proxy.clone())?;

        let mut env = self
            .l10n_resources
            .map(|it| Env::with_i10n(it.0, &it.1))
            .unwrap_or_else(Env::with_default_i10n);

        if let Some(f) = self.env_setup.take() {
            f(&mut env, &data);
        }

        let mut state = AppState::new(
            app.clone(),
            data,
            env,
            self.delegate.take(),
            self.ext_event_host,
            event_proxy,
        );

        for desc in self.windows {
            let window = desc.build_native(&mut state, &event_loop)?;
            window.show();
        }

        let mut timer_tokens = BTreeMap::new();

        event_loop.run(move |event, event_loop, control_flow| match event {
            glutin::event::Event::NewEvents(cause) => match cause {
                glutin::event::StartCause::Init => {
                    state.do_window_event(Event::ApplicationInit, WindowId::next());
                    *control_flow = ControlFlow::Wait;
                }
                glutin::event::StartCause::ResumeTimeReached {
                    start,
                    requested_resume,
                } => {
                    if let Some((window_id, token)) = timer_tokens.remove(&requested_resume) {
                        state.do_winit_window_event(Event::Timer(token), window_id);
                    }
                    if let Some(instant) = timer_tokens.keys().next() {
                        *control_flow = ControlFlow::WaitUntil(*instant);
                    } else {
                        *control_flow = ControlFlow::Wait;
                    }
                }
                glutin::event::StartCause::WaitCancelled {
                    start,
                    requested_resume,
                } => {}
                _ => (),
            },
            glutin::event::Event::MainEventsCleared => {}
            glutin::event::Event::RedrawEventsCleared => {}
            glutin::event::Event::UserEvent(event) => match event {
                WinitEvent::NewWindow => {
                    state.create_new_windows(event_loop);
                }
                WinitEvent::Idle(token) => {
                    state.idle(token);
                }
                WinitEvent::Timer(window_id, token, deadline) => {
                    let instant = std::time::Instant::now() + deadline;
                    timer_tokens.insert(instant, (window_id, token));
                    let instant = timer_tokens.keys().next().unwrap();
                    *control_flow = ControlFlow::WaitUntil(*instant);
                }
            },
            glutin::event::Event::LoopDestroyed => {
                state.do_window_event(Event::ApplicationQuit, WindowId::next());
            }
            glutin::event::Event::WindowEvent { window_id, event } => match event {
                glutin::event::WindowEvent::ScaleFactorChanged {
                    scale_factor,
                    new_inner_size,
                } => {
                    let size = Size::new(new_inner_size.width.into(), new_inner_size.height.into());
                    let event = Event::WindowSize(size, Some(scale_factor));
                    state.do_winit_window_event(event, window_id);
                }
                glutin::event::WindowEvent::CloseRequested => {
                    state.request_close_wint_window(window_id);
                    #[cfg(not(target_os = "macos"))]
                    if state.windows_count() == 0 {
                        *control_flow = ControlFlow::Exit;
                    }
                }
                glutin::event::WindowEvent::Moved(pos) => {
                    let scale = state.get_scale(window_id).unwrap_or(1.0);
                    let pos = Point::new(pos.x as f64 / scale, pos.y as f64 / scale);
                    let event = Event::WindowMoved(pos);
                    state.do_winit_window_event(event, window_id);
                }
                glutin::event::WindowEvent::Resized(size) => {
                    let size = Size::new(size.width.into(), size.height.into());
                    let scale = state.get_scale(window_id).unwrap_or(1.0);
                    let event = Event::WindowSize(size, Some(scale));
                    state.do_winit_window_event(event, window_id);
                }
                glutin::event::WindowEvent::ModifiersChanged(winit_mods) => {
                    let mut mods = Modifiers::empty();
                    if winit_mods.shift_key() {
                        mods.set(Modifiers::SHIFT, true);
                    }
                    if winit_mods.control_key() {
                        mods.set(Modifiers::CONTROL, true);
                    }
                    if winit_mods.alt_key() {
                        mods.set(Modifiers::ALT, true);
                    }
                    if winit_mods.super_key() {
                        mods.set(Modifiers::META, true);
                    }

                    state.set_mods(window_id, mods);
                }
                glutin::event::WindowEvent::MouseWheel { delta, .. } => {
                    let scale = state.get_scale(window_id).unwrap_or(1.0);
                    let mods = state.get_mods(window_id).unwrap_or(Modifiers::empty());
                    let buttons = state
                        .get_mouse_buttons(window_id)
                        .unwrap_or(MouseButtons::new());
                    let delta = match delta {
                        glutin::event::MouseScrollDelta::LineDelta(x, y) => {
                            Vec2::new(x as f64 * 32.0, -y as f64 * 32.0)
                        }
                        glutin::event::MouseScrollDelta::PixelDelta(pos) => {
                            Vec2::new(pos.x / scale, -pos.y / scale)
                        }
                    };
                    let pos = state.get_mouse_pos(window_id).unwrap_or(Point::ZERO);
                    let mouse_event = MouseEvent {
                        pos,
                        window_pos: pos,
                        buttons,
                        mods,
                        count: 0,
                        focus: false,
                        button: MouseButton::None,
                        wheel_delta: delta,
                    };
                    let event = Event::Wheel(mouse_event);
                    state.do_winit_window_event(event, window_id);
                }
                glutin::event::WindowEvent::CursorMoved {
                    device_id,
                    position,
                    modifiers,
                } => {
                    let scale = state.get_scale(window_id).unwrap_or(1.0);
                    let mods = if let Some(mods) = state.get_mods(window_id) {
                        mods
                    } else {
                        Modifiers::empty()
                    };
                    let pos = Point::new(position.x / scale, position.y / scale);
                    let buttons = state
                        .get_mouse_buttons(window_id)
                        .unwrap_or(MouseButtons::new());
                    let mouse_event = MouseEvent {
                        pos,
                        window_pos: pos,
                        buttons,
                        mods,
                        count: 0,
                        focus: false,
                        button: MouseButton::None,
                        wheel_delta: Vec2::ZERO,
                    };
                    let event = Event::MouseMove(mouse_event);
                    state.do_winit_window_event(event, window_id);
                }
                glutin::event::WindowEvent::MouseInput {
                    device_id,
                    state: mouse_state,
                    button,
                    modifiers,
                } => {
                    let mods = if let Some(mods) = state.get_mods(window_id) {
                        mods
                    } else {
                        Modifiers::empty()
                    };
                    let pos = state.get_mouse_pos(window_id).unwrap_or(Point::ZERO);
                    let mut buttons = state
                        .get_mouse_buttons(window_id)
                        .unwrap_or(MouseButtons::new());
                    let button = match button {
                        glutin::event::MouseButton::Left => MouseButton::Left,
                        glutin::event::MouseButton::Right => MouseButton::Right,
                        glutin::event::MouseButton::Middle => MouseButton::Middle,
                        glutin::event::MouseButton::Other(_) => MouseButton::None,
                    };
                    match mouse_state {
                        glutin::event::ElementState::Pressed => buttons.insert(button),
                        glutin::event::ElementState::Released => buttons.remove(button),
                    }
                    let mouse_event = MouseEvent {
                        pos,
                        window_pos: pos,
                        buttons,
                        mods,
                        count: 0,
                        focus: false,
                        button,
                        wheel_delta: Vec2::ZERO,
                    };
                    let event = match mouse_state {
                        glutin::event::ElementState::Pressed => Event::MouseDown(mouse_event),
                        glutin::event::ElementState::Released => Event::MouseUp(mouse_event),
                    };
                    state.do_winit_window_event(event, window_id);
                }
                glutin::event::WindowEvent::KeyboardInput {
                    event,
                    device_id,
                    is_synthetic,
                } => {
                    let mods = if let Some(mods) = state.get_mods(window_id) {
                        mods
                    } else {
                        Modifiers::empty()
                    };
                    let key_state = match event.state {
                        glutin::event::ElementState::Pressed => KeyState::Down,
                        glutin::event::ElementState::Released => KeyState::Up,
                    };
                    let mut key_event = KeyEvent::default();
                    key_event.state = key_state;
                    key_event.key = glutin_key(event.logical_key);
                    key_event.code = event.physical_key;
                    key_event.mods = mods;
                    key_event.repeat = event.repeat;
                    let event = match key_event.state {
                        KeyState::Down => Event::KeyDown(key_event),
                        KeyState::Up => Event::KeyUp(key_event),
                    };
                    state.do_winit_window_event(event, window_id);
                }
                _ => (),
            },
            glutin::event::Event::RedrawRequested(window_id) => {
                state.paint_winit_window(window_id);
            }
            _ => (),
        });

        // let handler = AppHandler::new(state);
        // app.run(Some(Box::new(handler)));
        // Ok(())
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        WindowConfig {
            size_policy: WindowSizePolicy::User,
            size: None,
            min_size: None,
            position: None,
            resizable: None,
            show_titlebar: None,
            set_title: None,
            transparent: None,
            level: None,
            state: None,
        }
    }
}

impl WindowConfig {
    /// Set the window size policy.
    pub fn window_size_policy(mut self, size_policy: WindowSizePolicy) -> Self {
        #[cfg(windows)]
        {
            // On Windows content_insets doesn't work on window with no initial size
            // so the window size can't be adapted to the content, to fix this a
            // non null initial size is set here.
            if size_policy == WindowSizePolicy::Content {
                self.size = Some(Size::new(1., 1.))
            }
        }
        self.size_policy = size_policy;
        self
    }

    /// Set the window's initial drawing area size in [display points].
    ///
    /// You can pass in a tuple `(width, height)` or a [`Size`],
    /// e.g. to create a window with a drawing area 1000dp wide and 500dp high:
    ///
    /// ```ignore
    /// window.window_size((1000.0, 500.0));
    /// ```
    ///
    /// The actual window size in pixels will depend on the platform DPI settings.
    ///
    /// This should be considered a request to the platform to set the size of the window.
    /// The platform might increase the size a tiny bit due to DPI.
    ///
    /// [`Size`]: struct.Size.html
    /// [display points]: struct.Scale.html
    pub fn window_size(mut self, size: impl Into<Size>) -> Self {
        self.size = Some(size.into());
        self
    }

    /// Set the window's minimum drawing area size in [display points].
    ///
    /// The actual minimum window size in pixels will depend on the platform DPI settings.
    ///
    /// This should be considered a request to the platform to set the minimum size of the window.
    /// The platform might increase the size a tiny bit due to DPI.
    ///
    /// To set the window's initial drawing area size use [`window_size`].
    ///
    /// [`window_size`]: #method.window_size
    /// [display points]: struct.Scale.html
    pub fn with_min_size(mut self, size: impl Into<Size>) -> Self {
        self.min_size = Some(size.into());
        self
    }

    /// Set whether the window should be resizable.
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = Some(resizable);
        self
    }

    /// Set whether the window should have a titlebar and decorations.
    pub fn show_titlebar(mut self, show_titlebar: bool) -> Self {
        self.show_titlebar = Some(show_titlebar);
        self
    }

    /// Set the title
    pub fn set_title(mut self, title: String) -> Self {
        self.set_title = Some(title);
        self
    }

    /// Sets the window position in virtual screen coordinates.
    /// [`position`] Position in pixels.
    ///
    /// [`position`]: struct.Point.html
    pub fn set_position(mut self, position: Point) -> Self {
        self.position = Some(position);
        self
    }

    /// Sets the [`WindowLevel`] of the window
    ///
    /// [`WindowLevel`]: enum.WindowLevel.html
    pub fn set_level(mut self, level: WindowLevel) -> Self {
        self.level = Some(level);
        self
    }

    /// Sets the [`WindowState`] of the window.
    ///
    /// [`WindowState`]: enum.WindowState.html
    pub fn set_window_state(mut self, state: WindowState) -> Self {
        self.state = Some(state);
        self
    }

    /// Set whether the window background should be transparent
    pub fn transparent(mut self, transparent: bool) -> Self {
        self.transparent = Some(transparent);
        self
    }

    /// Apply this window configuration to the passed in WindowBuilder
    pub fn apply_to_builder(&self, builder: WindowBuilder) -> WindowBuilder {
        let mut builder = if let Some(resizable) = self.resizable {
            builder.resizable(resizable)
        } else {
            builder
        };

        let builder = if let Some(show_titlebar) = self.show_titlebar {
            builder.show_titlebar(show_titlebar)
        } else {
            builder
        };

        let builder = if let Some(size) = self.size {
            builder.set_size(size)
        } else if let WindowSizePolicy::Content = self.size_policy {
            builder.set_size(Size::new(0., 0.))
        } else {
            builder
        };

        let mut builder = if let Some(position) = self.position {
            builder.set_position(position)
        } else {
            builder
        };

        if let Some(transparent) = self.transparent {
            builder.set_transparent(transparent);
        }

        if let Some(level) = self.level {
            builder.set_level(level)
        }

        let builder = if let Some(state) = self.state {
            builder.set_window_state(state)
        } else {
            builder
        };

        if let Some(min_size) = self.min_size {
            builder.set_min_size(min_size)
        } else {
            builder
        }
    }

    /// Apply this window configuration to the passed in WindowHandle
    pub fn apply_to_handle(&self, win_handle: &mut WindowHandle) {
        if let Some(resizable) = self.resizable {
            win_handle.resizable(resizable);
        }

        if let Some(show_titlebar) = self.show_titlebar {
            win_handle.show_titlebar(show_titlebar);
        }

        if let Some(title) = &self.set_title {
            win_handle.set_title(title);
        }

        if let Some(size) = self.size {
            win_handle.set_size(size);
        }

        // Can't apply min size currently as window handle
        // does not support it.

        if let Some(position) = self.position {
            win_handle.set_position(position);
        }

        if let Some(level) = self.level {
            win_handle.set_level(level)
        }

        if let Some(state) = self.state {
            win_handle.set_window_state(state);
        }
    }
}

impl<T: Data> WindowDesc<T> {
    /// Create a new `WindowDesc`, taking the root [`Widget`] for this window.
    ///
    /// [`Widget`]: trait.Widget.html
    pub fn new<W>(root: W) -> WindowDesc<T>
    where
        W: Widget<T> + 'static,
    {
        WindowDesc {
            pending: PendingWindow::new(root),
            config: WindowConfig::default(),
            id: WindowId::next(),
        }
    }

    pub fn new_with_id<W>(id: WindowId, root: W) -> WindowDesc<T>
    where
        W: Widget<T> + 'static,
    {
        WindowDesc {
            pending: PendingWindow::new(root),
            config: WindowConfig::default(),
            id,
        }
    }

    /// Set the title for this window. This is a [`LabelText`]; it can be either
    /// a `String`, a [`LocalizedString`], or a closure that computes a string;
    /// it will be kept up to date as the application's state changes.
    ///
    /// [`LabelText`]: widget/enum.LocalizedString.html
    /// [`LocalizedString`]: struct.LocalizedString.html
    pub fn title(mut self, title: impl Into<LabelText<T>>) -> Self {
        self.pending = self.pending.title(title);
        self
    }

    /// Set the menu for this window.
    ///
    /// `menu` is a callback for creating the menu. Its first argument is the id of the window that
    /// will have the menu, or `None` if it's creating the root application menu for an app with no
    /// menus (which can happen, for example, on macOS).
    pub fn menu(
        mut self,
        menu: impl FnMut(Option<WindowId>, &T, &Env) -> Menu<T> + 'static,
    ) -> Self {
        self.pending = self.pending.menu(menu);
        self
    }

    /// Set the window size policy
    pub fn window_size_policy(mut self, size_policy: WindowSizePolicy) -> Self {
        #[cfg(windows)]
        {
            // On Windows content_insets doesn't work on window with no initial size
            // so the window size can't be adapted to the content, to fix this a
            // non null initial size is set here.
            if size_policy == WindowSizePolicy::Content {
                self.config.size = Some(Size::new(1., 1.))
            }
        }
        self.config.size_policy = size_policy;
        self
    }

    /// Set the window's initial drawing area size in [display points].
    ///
    /// You can pass in a tuple `(width, height)` or a [`Size`],
    /// e.g. to create a window with a drawing area 1000dp wide and 500dp high:
    ///
    /// ```ignore
    /// window.window_size((1000.0, 500.0));
    /// ```
    ///
    /// The actual window size in pixels will depend on the platform DPI settings.
    ///
    /// This should be considered a request to the platform to set the size of the window.
    /// The platform might increase the size a tiny bit due to DPI.
    ///
    /// [`Size`]: struct.Size.html
    /// [display points]: struct.Scale.html
    pub fn window_size(mut self, size: impl Into<Size>) -> Self {
        self.config.size = Some(size.into());
        self
    }

    /// Set the window's minimum drawing area size in [display points].
    ///
    /// The actual minimum window size in pixels will depend on the platform DPI settings.
    ///
    /// This should be considered a request to the platform to set the minimum size of the window.
    /// The platform might increase the size a tiny bit due to DPI.
    ///
    /// To set the window's initial drawing area size use [`window_size`].
    ///
    /// [`window_size`]: #method.window_size
    /// [display points]: struct.Scale.html
    pub fn with_min_size(mut self, size: impl Into<Size>) -> Self {
        self.config = self.config.with_min_size(size);
        self
    }

    /// Builder-style method to set whether this window can be resized.
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.config = self.config.resizable(resizable);
        self
    }

    /// Builder-style method to set whether this window's titlebar is visible.
    pub fn show_titlebar(mut self, show_titlebar: bool) -> Self {
        self.config = self.config.show_titlebar(show_titlebar);
        self
    }

    /// Builder-style method to set whether this window's background should be
    /// transparent.
    pub fn transparent(mut self, transparent: bool) -> Self {
        self.config = self.config.transparent(transparent);
        self.pending = self.pending.transparent(transparent);
        self
    }

    /// Sets the initial window position in [display points], relative to the origin
    /// of the [virtual screen].
    ///
    /// [display points]: crate::Scale
    /// [virtual screen]: crate::Screen
    pub fn set_position(mut self, position: impl Into<Point>) -> Self {
        self.config = self.config.set_position(position.into());
        self
    }

    /// Sets the [`WindowLevel`] of the window
    ///
    /// [`WindowLevel`]: enum.WindowLevel.html
    pub fn set_level(mut self, level: WindowLevel) -> Self {
        self.config = self.config.set_level(level);
        self
    }

    /// Set initial state for the window.
    pub fn set_window_state(mut self, state: WindowState) -> Self {
        self.config = self.config.set_window_state(state);
        self
    }

    /// Set the [`WindowConfig`] of window.
    pub fn with_config(mut self, config: WindowConfig) -> Self {
        self.config = config;
        self
    }

    /// Attempt to create a platform window from this `WindowDesc`.
    pub(crate) fn build_native(
        self,
        state: &AppState<T>,
        window_target: &EventLoopWindowTarget<WinitEvent>,
    ) -> Result<WindowHandle, PlatformError> {
        state.build_native_window(self.id, self.pending, self.config, window_target)
    }
}
