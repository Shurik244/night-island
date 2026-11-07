use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::TaskScheduler::{
    ITaskFolder, ITaskService, TASK_CREATE_OR_UPDATE, TASK_LOGON_INTERACTIVE_TOKEN, TaskScheduler,
};
use windows::Win32::System::Variant::VARIANT;
use windows::core::{BSTR, Result};

pub struct Session {
    folder: ITaskFolder,
    user: String,
}

fn escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

fn definition(user: &str, executable: &str, directory: &str) -> String {
    let (user, executable, directory) = (escape(user), escape(executable), escape(directory));
    format!(
        r#"<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
<RegistrationInfo><Description>Start Night Island when this user signs in.</Description></RegistrationInfo>
<Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId><Delay>PT0S</Delay></LogonTrigger></Triggers>
<Principals><Principal id="User"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
<Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><StartWhenAvailable>true</StartWhenAvailable><ExecutionTimeLimit>PT0S</ExecutionTimeLimit><Enabled>true</Enabled></Settings>
<Actions Context="User"><Exec><Command>{executable}</Command><Arguments>--startup</Arguments><WorkingDirectory>{directory}</WorkingDirectory></Exec></Actions>
</Task>"#
    )
}

impl Session {
    pub fn connect() -> Result<Self> {
        unsafe {
            let service: ITaskService = CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)?;
            let empty = VARIANT::default();
            service.Connect(&empty, &empty, &empty, &empty)?;
            let user = format!("{}\\{}", service.ConnectedDomain()?, service.ConnectedUser()?);
            Ok(Self { folder: service.GetFolder(&BSTR::from("\\"))?, user })
        }
    }

    pub fn name(&self) -> String {
        use sha2::{Digest, Sha256};
        let suffix = format!("{:x}", Sha256::digest(self.user.as_bytes()));
        format!("NightIsland.{suffix}")
    }

    pub fn enabled(&self, name: &str) -> Result<bool> {
        unsafe { Ok(self.folder.GetTask(&BSTR::from(name))?.Enabled()?.as_bool()) }
    }

    pub fn install(&self, name: &str, executable: &std::path::Path) -> Result<()> {
        let directory = executable.parent().unwrap_or(executable);
        let xml = definition(&self.user, &executable.to_string_lossy(), &directory.to_string_lossy());
        let empty = VARIANT::default();
        unsafe {
            self.folder.RegisterTask(
                &BSTR::from(name),
                &BSTR::from(xml),
                TASK_CREATE_OR_UPDATE.0,
                &VARIANT::from(self.user.as_str()),
                &empty,
                TASK_LOGON_INTERACTIVE_TOKEN,
                &empty,
            )?;
        }
        Ok(())
    }

    pub fn remove(&self, name: &str) -> Result<()> {
        unsafe {
            match self.folder.DeleteTask(&BSTR::from(name), 0) {
                Err(error) if error.code().0 as u32 == 0x80070002 => Ok(()),
                result => result,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logon_task_is_interactive_immediate_and_escapes_user_paths() {
        let xml = definition("PC\\A&B", "C:\\Apps & tools\\Island.exe", "C:\\Apps & tools");
        assert!(xml.contains("PC\\A&amp;B"));
        assert!(xml.contains("<Delay>PT0S</Delay>"));
        assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"));
        assert!(xml.contains("<RunLevel>LeastPrivilege</RunLevel>"));
        assert!(xml.contains("<Arguments>--startup</Arguments>"));
        assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(xml.contains("<DisallowStartIfOnBatteries>false"));
    }

    #[test]
    #[ignore = "Creates and deletes an isolated per-user logon task without running it"]
    fn native_logon_task_registers_without_elevation() -> Result<()> {
        use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }
        struct Com;
        impl Drop for Com {
            fn drop(&mut self) {
                unsafe {
                    CoUninitialize();
                }
            }
        }
        let _com = Com;
        let session = Session::connect()?;
        let name = format!("{}.Test.{}", session.name(), std::process::id());
        struct Cleanup<'a>(&'a Session, &'a str);
        impl Drop for Cleanup<'_> {
            fn drop(&mut self) {
                let _ = self.0.remove(self.1);
            }
        }
        let _cleanup = Cleanup(&session, &name);
        session.install(&name, &std::env::current_exe().unwrap())?;
        assert!(session.enabled(&name)?);
        let xml = unsafe { session.folder.GetTask(&BSTR::from(name.as_str()))?.Xml()? }.to_string();
        assert!(xml.contains("--startup"));
        use windows::core::Interface;
        let mut delay = BSTR::default();
        unsafe {
            let trigger: windows::Win32::System::TaskScheduler::ILogonTrigger =
                session.folder.GetTask(&BSTR::from(name.as_str()))?.Definition()?.Triggers()?.get_Item(1)?.cast()?;
            trigger.Delay(&mut delay)?;
        }
        assert!(
            matches!(delay.to_string().as_str(), "" | "PT0S" | "PT0M" | "PT0H" | "P0D"),
            "Unexpected logon delay: {delay}"
        );
        session.remove(&name)?;
        assert!(session.enabled(&name).is_err());
        Ok(())
    }
}
