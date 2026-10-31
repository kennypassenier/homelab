//! Mounts and devices a freshly provisioned container needs, whichever way
//! it was made (create or clone).

use crate::error::CoreError;
use crate::executor::{Cmd, run_ok};

use super::run::Run;

impl Run<'_, '_> {
    /// The stack's own storage, then M1's borrowed directories continuing
    /// the same numbering.
    pub(super) async fn attach_mounts(&self) -> Result<(), CoreError> {
        let (_, _, m, exec) = self.parts();
        let vm = m.vmid.to_string();
        for (i, mount) in m.storage.iter().enumerate() {
            let mp = format!("-mp{}", i);
            let val = format!("{},mp={}", mount.host_path, mount.mount_point);
            run_ok(exec, &Cmd::new("pct", &["set", &vm, &mp, &val], 60)).await?;
        }
        for (i, dm) in m.data_mounts.iter().enumerate() {
            let mp = format!("-mp{}", m.storage.len() + i);
            let val = format!("{},mp={}", dm.host_path, dm.mount_point);
            run_ok(exec, &Cmd::new("pct", &["set", &vm, &mp, &val], 60)).await?;
        }
        Ok(())
    }

    /// H4: hardware passthrough flags. GPU via pct dev entries
    /// (targeted gids — render/video — NOT the old ansible
    /// chmod-0777-recurse). TUN needs raw lxc config lines that pct
    /// has no flag for, appended to the container config.
    pub(super) async fn attach_devices(&self) -> Result<(), CoreError> {
        let (_, _, m, exec) = self.parts();
        let vm = m.vmid.to_string();
        if let Some(g) = &self.gpu {
            let (dev0, dev1) = crate::ops::hardware::dev_args(g);
            run_ok(
                exec,
                &Cmd::new("pct", &["set", &vm, "--dev0", &dev0, "--dev1", &dev1], 60),
            )
            .await?;
        }
        if m.lxc.vpn {
            let conf_path = format!("/etc/pve/lxc/{}.conf", m.vmid);
            let conf = exec.read_file(&conf_path).await?;
            if !conf.contains("dev/net/tun") {
                let extra = "lxc.cgroup2.devices.allow: c 10:200 rwm\nlxc.mount.entry: /dev/net/tun dev/net/tun none bind,create=file\n";
                exec.write_file(&conf_path, &format!("{}{}", conf, extra), 0o640)
                    .await?;
            }
        }
        Ok(())
    }
}
