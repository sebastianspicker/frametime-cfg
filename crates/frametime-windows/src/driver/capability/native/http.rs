use std::{ffi::c_void, mem::size_of};

use windows::{
    Win32::{
        Foundation::HANDLE,
        Networking::WinHttp::{
            INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            WINHTTP_DISABLE_REDIRECTS, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER,
            WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen,
            WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
            WinHttpSendRequest, WinHttpSetOption,
        },
    },
    core::PCWSTR,
};

use super::*;

pub(super) struct Http(pub(super) *mut c_void);

impl Drop for Http {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

pub(super) fn http_open() -> Result<Http, AdapterFailure> {
    let agent = wide("frametime-cfg/3");
    let session = Http(unsafe {
        WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            None,
            None,
            0,
        )
    });
    if session.0.is_null() {
        Err(adapter("download NVIDIA artifact", "WinHttpOpen failed"))
    } else {
        Ok(session)
    }
}

pub(super) fn download_to_handle(
    host: &NvidiaDownloadHost,
    path: &str,
    file: HANDLE,
    maximum: usize,
) -> Result<(), AdapterFailure> {
    let session = http_open()?;
    let connection = connect_https(&session, host)?;
    let request = open_get_request(&connection, path)?;
    send_and_verify_response(&request)?;
    stream_response_to_file(&request, file, maximum)
}

fn connect_https(session: &Http, host: &NvidiaDownloadHost) -> Result<Http, AdapterFailure> {
    let authority = wide(host.authority());
    let connection = Http(unsafe {
        WinHttpConnect(
            session.0,
            PCWSTR(authority.as_ptr()),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        )
    });
    if connection.0.is_null() {
        Err(adapter("download NVIDIA artifact", "WinHttpConnect failed"))
    } else {
        Ok(connection)
    }
}

fn open_get_request(connection: &Http, path: &str) -> Result<Http, AdapterFailure> {
    let get = wide("GET");
    let resource = wide(path);
    let request = Http(unsafe {
        WinHttpOpenRequest(
            connection.0,
            PCWSTR(get.as_ptr()),
            PCWSTR(resource.as_ptr()),
            None,
            None,
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    });
    if request.0.is_null() {
        Err(adapter(
            "download NVIDIA artifact",
            "WinHttpOpenRequest failed",
        ))
    } else {
        Ok(request)
    }
}

fn send_and_verify_response(request: &Http) -> Result<(), AdapterFailure> {
    unsafe {
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_DISABLE_REDIRECTS,
            Some(&[1, 0, 0, 0]),
        )
    }
    .map_err(|e| adapter("download NVIDIA artifact", e.to_string()))?;
    unsafe { WinHttpSendRequest(request.0, None, None, 0, 0, 0) }
        .map_err(|e| adapter("download NVIDIA artifact", e.to_string()))?;
    unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) }
        .map_err(|e| adapter("download NVIDIA artifact", e.to_string()))?;
    let mut status = 0_u32;
    let mut length = u32::try_from(size_of::<u32>())
        .map_err(|_| adapter("download NVIDIA artifact", "header length exceeds u32"))?;
    let mut index = 0;
    unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            None,
            Some((&mut status as *mut u32).cast()),
            &mut length,
            &mut index,
        )
    }
    .map_err(|e| adapter("download NVIDIA artifact", e.to_string()))?;
    if status != 200 {
        return Err(adapter(
            "download NVIDIA artifact",
            format!("HTTPS response status {status} is not permitted"),
        ));
    }
    Ok(())
}

fn stream_response_to_file(
    request: &Http,
    file: HANDLE,
    maximum: usize,
) -> Result<(), AdapterFailure> {
    let mut total = 0_usize;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let mut read = 0;
        unsafe {
            WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                u32::try_from(buffer.len()).map_err(|_| {
                    adapter("download NVIDIA artifact", "buffer length exceeds u32")
                })?,
                &mut read,
            )
        }
        .map_err(|e| adapter("download NVIDIA artifact", e.to_string()))?;
        if read == 0 {
            return Ok(());
        }
        let count = usize::try_from(read).map_err(|_| {
            adapter(
                "download NVIDIA artifact",
                "read count exceeds address space",
            )
        })?;
        total = total
            .checked_add(count)
            .ok_or_else(|| adapter("download NVIDIA artifact", "response length overflow"))?;
        if total > maximum {
            return Err(adapter(
                "download NVIDIA artifact",
                "response exceeds bounded policy",
            ));
        }
        let mut written = 0;
        unsafe { WriteFile(file, Some(&buffer[..count]), Some(&mut written), None) }
            .map_err(|e| adapter("publish artifact", e.to_string()))?;
        let written = usize::try_from(written)
            .map_err(|_| adapter("publish artifact", "write count exceeds address space"))?;
        if written != count {
            return Err(adapter(
                "publish artifact",
                "short write of artifact stream",
            ));
        }
    }
}
