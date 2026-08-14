use super::*;

pub(super) fn layout(window: HWND) {
    let Some(controls) = with_state(window, |app| LayoutControls::from(app)) else {
        return;
    };
    let mut rect = RECT::default();
    unsafe {
        GetClientRect(window, &mut rect).expect("client rect");
    }
    let metrics = LayoutMetrics::new(window, rect);
    layout_navigation(&controls, metrics);
    layout_header(&controls, metrics);
    layout_primary_actions(&controls, metrics);
    layout_additional_actions(&controls, metrics);
    layout_catalog_filter(&controls, metrics);
    layout_benchmark_controls(&controls, metrics);
    layout_preference_controls(&controls, metrics);
    layout_video_controls(&controls, metrics);
    layout_table(&controls, metrics);
}

#[derive(Clone, Copy)]
struct LayoutControls {
    nav: [HWND; 7],
    heading: HWND,
    description: HWND,
    status: HWND,
    action: HWND,
    secondary: HWND,
    tertiary: HWND,
    quaternary: HWND,
    quinary: HWND,
    cancel: HWND,
    table: HWND,
    fps_label: HWND,
    fps_input: HWND,
    min_label: HWND,
    min_input: HWND,
    vprof_input: HWND,
    profile: HWND,
    dry_run: HWND,
    filter_label: HWND,
    catalog_filter: HWND,
    video_root_label: HWND,
    video_root: HWND,
    video_tier_label: HWND,
    video_tier: HWND,
    area: Area,
}

impl From<&mut AppState> for LayoutControls {
    fn from(app: &mut AppState) -> Self {
        Self {
            nav: app.nav,
            heading: app.heading,
            description: app.description,
            status: app.status,
            action: app.action,
            secondary: app.secondary,
            tertiary: app.tertiary,
            quaternary: app.quaternary,
            quinary: app.quinary,
            cancel: app.cancel,
            table: app.table,
            fps_label: app.fps_label,
            fps_input: app.fps_input,
            min_label: app.min_label,
            min_input: app.min_input,
            vprof_input: app.vprof_input,
            profile: app.profile,
            dry_run: app.dry_run,
            filter_label: app.filter_label,
            catalog_filter: app.catalog_filter,
            video_root_label: app.video_root_label,
            video_root: app.video_root,
            video_tier_label: app.video_tier_label,
            video_tier: app.video_tier,
            area: app.area,
        }
    }
}

#[derive(Clone, Copy)]
struct LayoutMetrics {
    margin: i32,
    content_x: i32,
    content_width: i32,
    client_bottom: i32,
    dpi: i32,
}

impl LayoutMetrics {
    fn new(window: HWND, rect: RECT) -> Self {
        let dpi = unsafe { GetDpiForWindow(window) }.max(96) as i32;
        let margin = Self::scale_for(dpi, 16);
        let nav_width = Self::scale_for(dpi, 176);
        let content_x = nav_width + margin * 2;
        Self {
            margin,
            content_x,
            content_width: (rect.right - content_x - margin).max(Self::scale_for(dpi, 360)),
            client_bottom: rect.bottom,
            dpi,
        }
    }

    const fn scale(self, value: i32) -> i32 {
        Self::scale_for(self.dpi, value)
    }

    const fn scale_for(dpi: i32, value: i32) -> i32 {
        value * dpi / 96
    }
}

fn layout_navigation(controls: &LayoutControls, metrics: LayoutMetrics) {
    let nav_width = metrics.scale(176);
    for (index, button) in controls.nav.iter().enumerate() {
        move_control(
            *button,
            metrics.margin,
            metrics.margin + index as i32 * metrics.scale(44),
            nav_width - metrics.margin * 2,
            metrics.scale(36),
        );
    }
}

fn layout_header(controls: &LayoutControls, metrics: LayoutMetrics) {
    move_control(
        controls.heading,
        metrics.content_x,
        metrics.margin,
        metrics.content_width,
        metrics.scale(28),
    );
    move_control(
        controls.description,
        metrics.content_x,
        metrics.margin + metrics.scale(36),
        metrics.content_width,
        metrics.scale(48),
    );
    move_control(
        controls.status,
        metrics.content_x,
        metrics.margin + metrics.scale(92),
        metrics.content_width,
        metrics.scale(40),
    );
}

fn layout_primary_actions(controls: &LayoutControls, metrics: LayoutMetrics) {
    let y = metrics.margin + metrics.scale(140);
    let button_width = metrics.scale(165);
    move_control(
        controls.action,
        metrics.content_x,
        y,
        button_width,
        metrics.scale(32),
    );
    move_control(
        controls.secondary,
        metrics.content_x + metrics.scale(175),
        y,
        button_width,
        metrics.scale(32),
    );
    move_control(
        controls.tertiary,
        metrics.content_x + metrics.scale(350),
        y,
        button_width,
        metrics.scale(32),
    );
    move_control(
        controls.cancel,
        metrics.content_x + metrics.scale(525),
        y,
        button_width,
        metrics.scale(32),
    );
}

fn layout_additional_actions(controls: &LayoutControls, metrics: LayoutMetrics) {
    let y = metrics.margin + metrics.scale(178);
    if matches!(
        controls.area,
        Area::Assess | Area::Benchmark | Area::Recovery
    ) {
        move_control(
            controls.quaternary,
            metrics.content_x,
            y,
            metrics.scale(165),
            metrics.scale(32),
        );
    }
    if controls.area == Area::Assess {
        move_control(
            controls.quinary,
            metrics.content_x + metrics.scale(175),
            y,
            metrics.scale(165),
            metrics.scale(32),
        );
    }
}

fn layout_catalog_filter(controls: &LayoutControls, metrics: LayoutMetrics) {
    let filter_y = if matches!(
        controls.area,
        Area::Assess | Area::Benchmark | Area::Recovery
    ) {
        216
    } else {
        178
    };
    move_control(
        controls.filter_label,
        metrics.content_x,
        metrics.margin + metrics.scale(filter_y + 3),
        metrics.scale(150),
        metrics.scale(24),
    );
    move_control(
        controls.catalog_filter,
        metrics.content_x + metrics.scale(155),
        metrics.margin + metrics.scale(filter_y),
        (metrics.content_width - metrics.scale(155)).max(metrics.scale(180)),
        metrics.scale(28),
    );
}

fn layout_benchmark_controls(controls: &LayoutControls, metrics: LayoutMetrics) {
    if controls.area != Area::Benchmark {
        return;
    }
    move_control(
        controls.fps_label,
        metrics.content_x,
        metrics.margin + metrics.scale(258),
        metrics.scale(88),
        metrics.scale(24),
    );
    move_control(
        controls.fps_input,
        metrics.content_x + metrics.scale(92),
        metrics.margin + metrics.scale(254),
        metrics.scale(96),
        metrics.scale(28),
    );
    move_control(
        controls.min_label,
        metrics.content_x + metrics.scale(202),
        metrics.margin + metrics.scale(258),
        metrics.scale(88),
        metrics.scale(24),
    );
    move_control(
        controls.min_input,
        metrics.content_x + metrics.scale(294),
        metrics.margin + metrics.scale(254),
        metrics.scale(80),
        metrics.scale(28),
    );
    move_control(
        controls.vprof_input,
        metrics.content_x,
        metrics.margin + metrics.scale(292),
        metrics.content_width,
        metrics.scale(74),
    );
}

fn layout_preference_controls(controls: &LayoutControls, metrics: LayoutMetrics) {
    if controls.area != Area::SetupVerify {
        return;
    }
    move_control(
        controls.profile,
        metrics.content_x,
        metrics.margin + metrics.scale(216),
        metrics.scale(180),
        metrics.scale(240),
    );
    move_control(
        controls.dry_run,
        metrics.content_x + metrics.scale(194),
        metrics.margin + metrics.scale(216),
        metrics.scale(190),
        metrics.scale(28),
    );
}

fn layout_video_controls(controls: &LayoutControls, metrics: LayoutMetrics) {
    if controls.area != Area::Video {
        return;
    }
    move_control(
        controls.video_root_label,
        metrics.content_x,
        metrics.margin + metrics.scale(220),
        metrics.scale(120),
        metrics.scale(24),
    );
    move_control(
        controls.video_root,
        metrics.content_x + metrics.scale(124),
        metrics.margin + metrics.scale(216),
        metrics.scale(270),
        metrics.scale(28),
    );
    move_control(
        controls.video_tier_label,
        metrics.content_x + metrics.scale(404),
        metrics.margin + metrics.scale(220),
        metrics.scale(76),
        metrics.scale(24),
    );
    move_control(
        controls.video_tier,
        metrics.content_x + metrics.scale(484),
        metrics.margin + metrics.scale(216),
        metrics.scale(140),
        metrics.scale(180),
    );
}

fn layout_table(controls: &LayoutControls, metrics: LayoutMetrics) {
    let table_y = metrics.margin
        + if controls.area == Area::Benchmark {
            metrics.scale(376)
        } else if matches!(
            controls.area,
            Area::Assess | Area::SetupVerify | Area::Recovery | Area::Video
        ) {
            metrics.scale(254)
        } else {
            metrics.scale(216)
        };
    move_control(
        controls.table,
        metrics.content_x,
        table_y,
        metrics.content_width,
        (metrics.client_bottom - table_y - metrics.margin).max(metrics.scale(150)),
    );
}

pub(super) fn move_control(handle: HWND, x: i32, y: i32, width: i32, height: i32) {
    unsafe {
        SetWindowPos(
            handle,
            None,
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
        .expect("layout standard control");
    }
}

pub(super) fn command_id(wparam: WPARAM) -> usize {
    wparam.0 & 0xffff
}
