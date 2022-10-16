extern crate glib;

use eyre::Result;
use libc;
use nm::{
    Client, ConnectionExt, IPAddress, SettingIP4Config, SettingIP6Config, SettingIPConfigExt,
    SETTING_IP4_CONFIG_METHOD_AUTO, SETTING_IP4_CONFIG_METHOD_MANUAL,
};

pub struct IPConfig {
    version: u32,
    address: String,
    gateway: String,
    dns: String,
    prefix: u32,
}

pub struct NetworkConfig {
    name: String,
    ipv4cfg: Option<IPConfig>,
    ipv6cfg: Option<IPConfig>,
}

impl NetworkConfig {
    /// Save the NetworkConfig to disk.
    pub async fn save(&self) -> Result<()> {
        let client = Client::new_future().await?;
        if let Some(connection) = client.connection_by_id(&self.name) {
            self.handle_ipv4(&connection)?;
            self.handle_ipv6(&connection)?;
            connection.commit_changes_future(true).await?;
        }
        Ok(())
    }

    /// Set address, dns and gateway from configuration to NetworkManager
    /// Connection.
    fn set_manual(nm_ipcfg: &impl SettingIPConfigExt, ipcfg: &IPConfig) -> Result<()> {
        nm_ipcfg.clear_addresses();
        nm_ipcfg.clear_dns();
        let new_addr: _;
        let mut inet = libc::AF_INET;
        if ipcfg.address.contains(":") {
            inet = libc::AF_INET6;
        }
        new_addr = IPAddress::new(inet, &ipcfg.address, ipcfg.prefix)?;
        nm_ipcfg.add_address(&new_addr);
        nm_ipcfg.set_gateway(Some(&ipcfg.gateway));
        nm_ipcfg.add_dns(&ipcfg.dns);
        nm_ipcfg.set_method(Some(&SETTING_IP4_CONFIG_METHOD_MANUAL));
        Ok(())
    }

    /// Set the NetworkManager Connection to DHCP.
    fn set_dhcp(nm_ipcfg: &impl SettingIPConfigExt) -> Result<()> {
        nm_ipcfg.clear_addresses();
        nm_ipcfg.clear_dns();
        nm_ipcfg.set_method(Some(&SETTING_IP4_CONFIG_METHOD_AUTO));
        Ok(())
    }

    /// Read configurations related to IPv4 from NetworkConfig, and update
    /// the NetworkManager connection.
    ///
    /// If the Ipv4 configuration is empty, use DHCP by default.
    fn handle_ipv4(&self, connection: &nm::RemoteConnection) -> Result<()> {
        match &self.ipv4cfg {
            Some(ipv4cfg) => {
                NetworkConfig::handle_manual(
                    &connection.setting_ip4_config(),
                    connection,
                    ipv4cfg,
                )?;
            }
            None => NetworkConfig::handle_dhcp(&connection.setting_ip4_config(), connection, 4)?,
        }
        Ok(())
    }

    /// Read configurations related to IPv6 from NetworkConfig, and update
    /// the NetworkManager connection.
    ///
    /// If the Ipv6 configuration is empty, use DHCP by default.
    fn handle_ipv6(&self, connection: &nm::RemoteConnection) -> Result<()> {
        match &self.ipv6cfg {
            Some(ipv6cfg) => {
                NetworkConfig::handle_manual(
                    &connection.setting_ip6_config(),
                    connection,
                    ipv6cfg,
                )?;
            }
            None => NetworkConfig::handle_dhcp(&connection.setting_ip6_config(), connection, 6)?,
        }
        Ok(())
    }

    /// Handle different ip protocol configuration for manual IP setting.
    fn handle_manual(
        nm_ipcfg: &Option<impl SettingIPConfigExt>,
        connection: &nm::RemoteConnection,
        ipcfg: &IPConfig,
    ) -> Result<()> {
        if let Some(nm_ipcfg_) = nm_ipcfg {
            NetworkConfig::set_manual(nm_ipcfg_, ipcfg)?;
        } else {
            if ipcfg.version == 4 {
                let nm_ipcfg = SettingIP4Config::new();
                NetworkConfig::set_manual(&nm_ipcfg, ipcfg)?;
                connection.add_setting(&nm_ipcfg);
            } else {
                let nm_ipcfg = SettingIP6Config::new();
                NetworkConfig::set_manual(&nm_ipcfg, ipcfg)?;
                connection.add_setting(&nm_ipcfg);
            }
        }
        Ok(())
    }

    /// Handle different ip protocol for DHCP setting.
    fn handle_dhcp(
        nm_ipcfg: &Option<impl SettingIPConfigExt>,
        connection: &nm::RemoteConnection,
        version: u32,
    ) -> Result<()> {
        if let Some(nm_ipcfg) = nm_ipcfg {
            NetworkConfig::set_dhcp(nm_ipcfg)?;
        } else {
            if version == 4 {
                let nm_ipcfg = SettingIP4Config::new();
                NetworkConfig::set_dhcp(&nm_ipcfg)?;
                connection.add_setting(&nm_ipcfg);
            } else {
                let nm_ipcfg = SettingIP6Config::new();
                NetworkConfig::set_dhcp(&nm_ipcfg)?;
                connection.add_setting(&nm_ipcfg);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_setting() -> Result<()> {
        let ctx = glib::MainContext::default();
        let gloop = glib::MainLoop::new(Some(&ctx), false);

        ctx.with_thread_default(|| {
            let l_clone = gloop.clone();
            let example = NetworkConfig {
                name: "CMCC-cGQe".to_string(),
                ipv4cfg: Some(IPConfig {
                    version: 4,
                    address: "192.168.233.233".to_string(),
                    gateway: "192.168.233.1".to_string(),
                    dns: "8.8.8.8".to_string(),
                    prefix: 32,
                }),
                ipv6cfg: Some(IPConfig {
                    version: 6,
                    address: "::1".to_string(),
                    gateway: "::1".to_string(),
                    dns: "::1".to_string(),
                    prefix: 64,
                }),
            };
            let future = async move {
                example.save().await.unwrap();
                l_clone.quit();
            };
            ctx.spawn_local(future);
            gloop.run();
        }).unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        Ok(())
    }
}
