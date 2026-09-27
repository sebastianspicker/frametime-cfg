use super::*;

pub(super) fn insert_columns(table: HWND) {
    for (index, (title, width)) in [("Item", 220), ("Value", 270), ("State", 250)]
        .iter()
        .enumerate()
    {
        let title = utf16(title);
        let mut column = LVCOLUMNW {
            mask: LVCF_TEXT | LVCF_WIDTH,
            cx: *width,
            pszText: windows::core::PWSTR(title.as_ptr().cast_mut()),
            ..Default::default()
        };
        unsafe {
            SendMessageW(
                table,
                LVM_INSERTCOLUMNW,
                Some(WPARAM(index)),
                Some(LPARAM(
                    (&mut column as *mut LVCOLUMNW).cast::<c_void>() as isize
                )),
            );
        }
    }
}
pub(super) fn populate_table(table: HWND, rows: &[(&str, &str, &str)]) {
    unsafe {
        SendMessageW(table, LVM_DELETEALLITEMS, Some(WPARAM(0)), Some(LPARAM(0)));
    }
    for (row_index, row) in rows.iter().enumerate() {
        let Ok(row_index_i32) = i32::try_from(row_index) else {
            return;
        };
        for (column_index, value) in [row.0, row.1, row.2].iter().enumerate() {
            let value = utf16(value);
            let mut item = LVITEMW {
                mask: LVIF_TEXT,
                iItem: row_index_i32,
                iSubItem: i32::try_from(column_index)
                    .expect("three fixed table columns fit in i32"),
                pszText: windows::core::PWSTR(value.as_ptr().cast_mut()),
                ..Default::default()
            };
            let message = if column_index == 0 {
                LVM_INSERTITEMW
            } else {
                LVM_SETITEMTEXTW
            };
            unsafe {
                SendMessageW(
                    table,
                    message,
                    Some(WPARAM(row_index)),
                    Some(LPARAM((&mut item as *mut LVITEMW).cast::<c_void>() as isize)),
                );
            }
        }
    }
}

pub(super) fn fit_columns(table: HWND, titles: [&str; 3]) {
    use windows::Win32::UI::Controls::LVM_SETCOLUMNW;
    let mut rect = RECT::default();
    if unsafe { GetClientRect(table, &mut rect) }.is_err() {
        return;
    }
    let width = (rect.right - 24).max(180);
    let columns = [width / 4, width * 3 / 8, width * 3 / 8];
    for (index, title) in titles.iter().enumerate() {
        let title = utf16(title);
        let column = LVCOLUMNW {
            mask: LVCF_TEXT | LVCF_WIDTH,
            cx: columns[index],
            pszText: windows::core::PWSTR(title.as_ptr().cast_mut()),
            ..Default::default()
        };
        unsafe {
            SendMessageW(
                table,
                LVM_SETCOLUMNW,
                Some(WPARAM(index)),
                Some(LPARAM(std::ptr::from_ref(&column) as isize)),
            );
        }
    }
}
