use super::*;

pub(in crate::app) fn place(window: HWND, top: i32, width: i32, height: i32, dpi: i32) {
    let Some((
        c,
        heading,
        description,
        status,
        input,
        raw_label,
        raw,
        table,
        filter_label,
        filter,
        stage,
        history,
    )) = with_state(window, |app| {
        (
            app.fps,
            app.heading,
            app.description,
            app.status,
            app.vprof_input,
            app.min_label,
            app.min_input,
            app.table,
            app.filter_label,
            app.catalog_filter,
            app.fps_session.stage(),
            app.fps_history,
        )
    })
    else {
        return;
    };
    let s = |n: i32| n * dpi / 96;
    let margin = s(16);
    let inner = width - margin * 2;
    let move_at = |control, x, y, w, h| move_control(control, margin + x, top + y, w, h);
    let step_width = inner / 4;
    for (index, step) in c.steps.iter().enumerate() {
        move_at(
            *step,
            i32::try_from(index).expect("fixed control count fits i32") * step_width,
            0,
            step_width - s(6),
            s(30),
        );
    }
    move_at(heading, 0, s(44), inner, s(34));
    move_at(description, 0, s(82), inner, s(48));
    let body = s(152);
    let bottom = height - top - s(112);
    let left_width = (inner - s(24)) / 2;
    let right_x = left_width + s(24);
    if history {
        move_at(filter_label, 0, body, s(155), s(24));
        move_at(filter, s(160), body, inner - s(160), s(28));
        move_at(
            table,
            0,
            body + s(38),
            inner,
            (bottom - body - s(42)).max(s(120)),
        );
    } else {
        place_body(
            c,
            &move_at,
            stage,
            BodyGeometry {
                body,
                bottom,
                inner,
                left: left_width,
                right: right_x,
            },
            s,
            [input, raw_label, raw, table],
        );
    }
    fit_columns(
        table,
        if history {
            ["Item", "Value", "State"]
        } else {
            ["Run", "Avg FPS", "P1 FPS"]
        },
    );
    move_at(c.history, 0, bottom + s(10), s(190), s(32));
    move_at(c.etw, s(200), bottom + s(10), s(195), s(32));
    move_at(
        c.back,
        (inner - s(360)).max(0),
        bottom + s(10),
        s(105),
        s(32),
    );
    move_at(c.next, inner - s(245), bottom + s(10), s(245), s(32));
    move_at(status, 0, bottom + s(52), inner, s(54));
}

struct BodyGeometry {
    body: i32,
    bottom: i32,
    inner: i32,
    left: i32,
    right: i32,
}

fn place_body(
    c: FpsControls,
    at: &impl Fn(HWND, i32, i32, i32, i32),
    stage: FpsStage,
    geometry: BodyGeometry,
    s: impl Fn(i32) -> i32,
    [input, raw_label, raw, table]: [HWND; 4],
) {
    let BodyGeometry {
        body,
        bottom,
        inner,
        left,
        right,
    } = geometry;
    at(
        c.group_left,
        -s(8),
        body - s(22),
        if stage == FpsStage::Import {
            inner + s(16)
        } else {
            left + s(16)
        },
        bottom - body + s(22),
    );
    at(
        c.group_right,
        right - s(8),
        body - s(22),
        left + s(16),
        bottom - body + s(22),
    );
    match stage {
        FpsStage::Import => {
            at(c.source, 0, body, inner - s(380), s(28));
            at(c.open, inner - s(370), body - s(4), s(160), s(32));
            at(c.paste, inner - s(200), body - s(4), s(200), s(32));
            at(
                input,
                0,
                body + s(40),
                inner,
                (bottom - body - s(82)).max(s(100)),
            );
            at(c.summary, 0, bottom - s(32), inner, s(28));
        }
        FpsStage::Evaluate => {
            at(table, 0, body, left, (bottom - body - s(8)).max(s(160)));
            at(c.strategy_label, right, body, s(110), s(26));
            at(
                c.strategy,
                right + s(120),
                body - s(4),
                left - s(120),
                s(160),
            );
            for (label, edit, offset) in [
                (c.refresh_label, c.refresh, 44),
                (c.margin_label, c.margin, 84),
                (raw_label, raw, 44),
            ] {
                at(label, right, body + s(offset), s(195), s(26));
                at(
                    edit,
                    right + s(200),
                    body + s(offset - 4),
                    left - s(200),
                    s(28),
                );
            }
            at(
                c.summary,
                right,
                body + s(130),
                left,
                (bottom - body - s(134)).max(s(110)),
            );
        }
        FpsStage::Review | FpsStage::Save | FpsStage::Saved => {
            at(c.summary, 0, body, left, s(190));
            at(
                c.permission,
                right,
                body,
                left,
                (bottom - body - s(65)).max(s(180)),
            );
            at(c.elevate, right, bottom - s(48), left, s(34));
            at(c.label_label, 0, bottom - s(74), left, s(26));
            at(c.label, 0, bottom - s(44), left, s(28));
        }
    }
}
