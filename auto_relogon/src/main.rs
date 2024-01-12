use std::{ptr, env, process::Command, fs};

use windows::{
    Win32::{System::{
    RemoteDesktop::{
        WTSEnumerateSessionsW, WTS_CURRENT_SERVER_HANDLE, WTS_SESSION_INFOW,
        WTSQuerySessionInformationW, WTSUserName, WTSFreeMemory,
        WTSIsRemoteSession
    },
    Com::{
        COINIT_MULTITHREADED, CoInitializeEx, CoCreateInstance,
        CLSCTX_INPROC_SERVER, CoInitializeSecurity, RPC_C_AUTHN_LEVEL_PKT_PRIVACY,
        RPC_C_IMP_LEVEL_IMPERSONATE, EOLE_AUTHENTICATION_CAPABILITIES
    }, 
    TaskScheduler::{TaskScheduler, ITaskService, TASK_CREATE_OR_UPDATE, TASK_LOGON_INTERACTIVE_TOKEN},
    Variant,
}, Foundation::{RPC_E_TOO_LATE, VARIANT_BOOL}}, core::{PWSTR, BSTR}};

const TASK_XML: &'static str = include_str!("task.xml");

fn get_os_program_path()->Result<String, String>{
    if let Some(system_drive) = env::var_os("SystemDrive") {
        return Ok(format!("{}\\Program Files", system_drive.to_string_lossy()));
    } else {
        return Err("Cant get environment variable 'SystemDrive'".to_owned());
    }
}

fn copy_software_to_system() -> String{
    let pkg_name = env!("CARGO_PKG_NAME");
    let os_prog_dir = get_os_program_path().unwrap();
    let program_dir = format!("{}\\{}", os_prog_dir, pkg_name);
    let dst = format!("{}\\{}.exe", program_dir, pkg_name);
    let src = env::current_exe().unwrap();
    fs::create_dir_all(program_dir).expect("cant create dir for program!");
    fs::copy(src, &dst).expect("cant copy program to program dir");
    return dst;
}

fn get_username_by_session_id(session_id: u32) -> windows::core::Result<String> {unsafe {
    let mut buffer: *mut u16 = std::ptr::null_mut();
    let mut bytes_returned: u32 = 0;
    WTSQuerySessionInformationW(
        WTS_CURRENT_SERVER_HANDLE,
        session_id,
        WTSUserName,
        (&mut buffer as *mut *mut u16).cast::<PWSTR>(),
        &mut bytes_returned,
    )?;

    let username = std::slice::from_raw_parts(buffer, (bytes_returned / 2) as usize - 1);
    let username_string: String = String::from_utf16_lossy(username);
    WTSFreeMemory(buffer as *mut _);
    Ok(username_string)
}}

fn is_remote_session(session:&WTS_SESSION_INFOW) -> windows::core::Result<bool>{unsafe {
    let mut buffer: *mut VARIANT_BOOL = std::ptr::null_mut();
    let mut bytes_returned: u32 = 0;
    WTSQuerySessionInformationW(
        WTS_CURRENT_SERVER_HANDLE,
        session.SessionId,
        WTSIsRemoteSession,
        (&mut buffer as *mut *mut VARIANT_BOOL).cast::<PWSTR>(),
        &mut bytes_returned,
    )?;
    
    let ret = *buffer == true;
    WTSFreeMemory(buffer as *mut _);
    Ok(ret)
}}

fn is_destoryed_session(session:&WTS_SESSION_INFOW) -> windows::core::Result<bool>{ unsafe{
    Ok(session.pWinStationName.to_string()?.trim().len() == 0)
}}

fn perform_relogon(session:u32, dest:&str) -> std::io::Result<()> {
    Command::new("tscon.exe")
        .args(&[&session.to_string(), &format!("/dest:{}", dest)])
        .spawn()?;
    Ok(())
}

fn relogon_user(user:&str, dest:&str) -> windows::core::Result<()> {
    let mut info: *mut WTS_SESSION_INFOW = ptr::null_mut();
    let mut count = 0u32;
    let session_infos = unsafe{
        WTSEnumerateSessionsW(WTS_CURRENT_SERVER_HANDLE, 0, 1, &mut info as *mut _, &mut count as *mut _)?;
        std::slice::from_raw_parts(info, count as usize)
    };
    println!("{:?}", session_infos);
    for session_info in session_infos {
        let session_user = get_username_by_session_id(session_info.SessionId)?;
        let is_remote = is_remote_session(session_info).unwrap_or(false);
        let is_destoryed = is_destoryed_session(session_info)?;
        if session_user.trim() == user.trim() && (is_remote || is_destoryed) {
            perform_relogon(session_info.SessionId, dest).unwrap();
        }
        println!("session: {}, user: {}, is remote: {}, is destoryed: {}", session_info.SessionId, session_user, is_remote, is_destoryed);
    }

    Ok(())
}

fn create_task(user:&str, dest:&str, exe_path:&str) -> windows::core::Result<()> {unsafe{
    let task_name = format!("AutoRelogon{}", user.replace(" ", ""));
    CoInitializeEx(None, COINIT_MULTITHREADED)?;
    let res = CoInitializeSecurity(
        None,
        -1,
        None,
        None,
        RPC_C_AUTHN_LEVEL_PKT_PRIVACY,
        RPC_C_IMP_LEVEL_IMPERSONATE,
        None,
        EOLE_AUTHENTICATION_CAPABILITIES(0),
        None);
    if let Err(e) = res{
        if e.code().0 != RPC_E_TOO_LATE.0{
            return Err(e)
        }
    }
    let service:ITaskService = CoCreateInstance(
        &TaskScheduler as *const _,
        None,
        CLSCTX_INPROC_SERVER,
    )?;
    service.Connect(Variant::VariantInit(),Variant::VariantInit(),Variant::VariantInit(),Variant::VariantInit())?;
    let root_folder = service.GetFolder(&BSTR::from("\\"))?;
    let task = service.NewTask(0)?;
    task.SetXmlText(&BSTR::from(TASK_XML
        .replace("{author}", "Task")
        .replace("{uri}", &format!("\\{}", task_name))
        .replace("{command}", exe_path)
        .replace("{arguments}", &format!("perform {} {}", user, dest))
    ))?;
    root_folder.RegisterTaskDefinition(
        &BSTR::from(task_name),  // Change this to the desired task name
        &task,
        TASK_CREATE_OR_UPDATE.0,
        Variant::VariantInit(),
        Variant::VariantInit(),
        TASK_LOGON_INTERACTIVE_TOKEN,
        Variant::VariantInit(),
    )?;

    Ok(())
}}

const USAGE_HINT:&'static str = 
r#"Usage:
auto_relogon set <username> <destination>       install auto logon setting for user.
auto_relogon unset <username>                   uninstall auto logon setting for user.
auto_relogon perform <username> <destination>   perform auto logon.
"#;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() == 4 && args[1] == "set" {
        let exe = copy_software_to_system();
        create_task(args[2].trim(), args[3].trim(), &exe).unwrap();
    }else if args.len() == 3 && args[1] == "unset" {
        todo!()
    }else if args.len() == 4 && args[1] == "perform" {
        relogon_user(args[2].trim(), args[3].trim()).unwrap();
    }else{
        println!("{}", USAGE_HINT);
    }
}