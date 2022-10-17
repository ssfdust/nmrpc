extern crate glib;

use super::nmmgr::MapNMManger;
use eyre::Result;
use glib::Cast;
use libc;
use nm::{
    Client, ConnectionExt, IPAddress, SettingIP4Config, SettingIP6Config, SettingIPConfig,
    SettingIPConfigExt, SETTING_IP4_CONFIG_METHOD_AUTO, SETTING_IP4_CONFIG_METHOD_MANUAL,
    SETTING_IP6_CONFIG_METHOD_AUTO, SETTING_IP6_CONFIG_METHOD_MANUAL,
};
use std::{fmt::Display, net::IpAddr};

/// The Ip configuration struct
#[derive(Debug)]
pub struct IPConfig {
    address: IpAddr,
    gateway: Option<IpAddr>,
    dns: Option<IpAddr>,
    prefix: u32,
}

impl Display for IPConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "address: {}/{}\ngateway: {}\ndns: {}",
            self.address,
            self.prefix,
            self.gateway.map_or(String::new(), |x| x.to_string()),
            self.dns.map_or(String::new(), |x| x.to_string())
        )
    }
}

/// A simple Network Config consists of connection name,
/// IpV4 configuration and IpV6 configuration.
#[derive(Debug)]
pub struct NetworkConfig {
    /// The connection name
    name: String,
    /// The IpV4 Config of the connection
    ipv4cfg: Option<IPConfig>,
    /// The IpV6 Config of the connection
    ipv6cfg: Option<IPConfig>,
}

impl Display for NetworkConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "\nname: {}\n\n===== ipv4 ===== \n{}\n===== ipv4 =====\n\n===== ipv6 =====\n{}\n===== ipv6 =====",
            self.name,
            self.ipv4cfg
                .as_ref()
                .map(|x| x.to_string())
                .unwrap_or(String::new()),
            self.ipv6cfg
                .as_ref()
                .map(|x| x.to_string())
                .unwrap_or(String::new())
        )
    }
}

impl IPConfig {
    fn from_settings(settings: SettingIPConfig) -> Option<Self> {
        match settings.method() {
            Some(val) if val == *SETTING_IP4_CONFIG_METHOD_MANUAL => {
                let addr: IpAddr;
                let prefix: u32;
                let gateway: Option<IpAddr>;
                let dns: Option<IpAddr>;
                if let Some((Some(address), prefix_)) = settings
                    .address(0)
                    .map(|ipaddr| (ipaddr.address(), ipaddr.prefix()))
                {
                    addr = address.parse().unwrap();
                    prefix = prefix_;
                    gateway = settings.gateway().map(|x| x.to_string().parse().unwrap());
                    dns = settings.dns(0).map(|x| x.to_string().parse().unwrap());
                    return Some(IPConfig {
                        address: addr,
                        prefix,
                        gateway,
                        dns,
                    });
                } else {
                    return None;
                };
            }
            _ => None,
        }
    }
}

impl NetworkConfig {
    pub async fn new_future(name: &str) -> Result<Self> {
        let mut mapnm_manger = MapNMManger::new_future(None).await?;
        let conn = mapnm_manger.connection_by_name(name).await?;
        let ipv4cfg = conn
            .setting_ip4_config()
            .map(|x| IPConfig::from_settings(x.upcast::<SettingIPConfig>()))
            .unwrap_or(None);
        let ipv6cfg = conn
            .setting_ip6_config()
            .map(|x| IPConfig::from_settings(x.upcast::<SettingIPConfig>()))
            .unwrap_or(None);
        Ok(NetworkConfig {
            name: name.to_string(),
            ipv4cfg,
            ipv6cfg,
        })
    }

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
        let inet: _;
        match ipcfg.address {
            IpAddr::V4(_) => {
                inet = libc::AF_INET;
                nm_ipcfg.set_method(Some(&SETTING_IP4_CONFIG_METHOD_MANUAL));
            }
            IpAddr::V6(_) => {
                inet = libc::AF_INET6;
                nm_ipcfg.set_method(Some(&SETTING_IP6_CONFIG_METHOD_MANUAL));
            }
        }
        new_addr = IPAddress::new(inet, &ipcfg.address.to_string(), ipcfg.prefix)?;
        nm_ipcfg.add_address(&new_addr);
        if let Some(gateway) = ipcfg.gateway {
            nm_ipcfg.add_dns(gateway.to_string().as_str());
        }
        if let Some(dns) = ipcfg.dns {
            nm_ipcfg.add_dns(dns.to_string().as_str());
        }
        Ok(())
    }

    /// Set the NetworkManager Connection to DHCP.
    fn set_dhcp(nm_ipcfg: &impl SettingIPConfigExt, version: u32) -> Result<()> {
        nm_ipcfg.clear_addresses();
        nm_ipcfg.clear_dns();
        nm_ipcfg.set_gateway(None);
        if version == 4 {
            nm_ipcfg.set_method(Some(&SETTING_IP4_CONFIG_METHOD_AUTO));
        } else {
            nm_ipcfg.set_method(Some(&SETTING_IP6_CONFIG_METHOD_AUTO));
        }
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
            match ipcfg.address {
                IpAddr::V4(_) => {
                    let nm_ipcfg = SettingIP4Config::new();
                    NetworkConfig::set_manual(&nm_ipcfg, ipcfg)?;
                    connection.add_setting(&nm_ipcfg);
                }
                IpAddr::V6(_) => {
                    let nm_ipcfg = SettingIP6Config::new();
                    NetworkConfig::set_manual(&nm_ipcfg, ipcfg)?;
                    connection.add_setting(&nm_ipcfg);
                }
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
            NetworkConfig::set_dhcp(nm_ipcfg, version)?;
        } else {
            if version == 4 {
                let nm_ipcfg = SettingIP4Config::new();
                NetworkConfig::set_dhcp(&nm_ipcfg, version)?;
                connection.add_setting(&nm_ipcfg);
            } else {
                let nm_ipcfg = SettingIP6Config::new();
                NetworkConfig::set_dhcp(&nm_ipcfg, version)?;
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
    async fn test_setting() {
        let ctx = glib::MainContext::default();
        loop {
            match ctx.with_thread_default(|| {
                let example = NetworkConfig {
                    name: "eth0".to_string(),
                    ipv4cfg: Some(IPConfig {
                        address: "192.168.233.233".parse().unwrap(),
                        gateway: Some("192.168.233.1".parse().unwrap()),
                        dns: Some("8.8.8.8".parse().unwrap()),
                        prefix: 32,
                    }),
                    ipv6cfg: None,
                };
                ctx.block_on(example.save()).unwrap();
                println!(
                    "{}",
                    ctx.block_on(NetworkConfig::new_future("eth0")).unwrap()
                );
            }) {
                Ok(_) => break,
                _ => (),
            }
        }
    }
}
