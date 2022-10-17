use eyre::Result;
use nm::{
    Client, ConnectionExt, DeviceExt, DeviceType, SettingConnection, SimpleConnection,
    SETTING_WIRED_SETTING_NAME,
};
use std::collections::{BTreeMap, VecDeque};

/// The NMManger takes an argument, which is used to
/// decide the relation ship between connection name
/// and device name.
pub struct MapNMManger {
    map: BTreeMap<String, String>,
    created_ifaces: VecDeque<String>,
}

/// Get a Network Mananger connection by name. If there's no
/// connection matches the name, it will create a new connection
/// with DHCP.
impl MapNMManger {
    /// Initilize a new MapNMManger instance.
    pub async fn new_future(map: Option<&BTreeMap<String, String>>) -> Result<Self> {
        match map {
            Some(val) => Ok(MapNMManger {
                map: val.clone(),
                created_ifaces: VecDeque::new(),
            }),
            _ => Ok(MapNMManger {
                map: Self::get_deafult_mapping().await?,
                created_ifaces: VecDeque::new(),
            }),
        }
    }

    /// Get default system devices mapping
    pub async fn get_deafult_mapping() -> Result<BTreeMap<String, String>> {
        let mut default_mapping = BTreeMap::new();
        let devices = Self::get_ether_devices().await?;
        for (idx, device) in devices.iter().enumerate() {
            default_mapping.insert(format!("eth{}", idx), format!("{}", device));
        }
        Ok(default_mapping)
    }

    /// Get all system devices
    pub async fn get_ether_devices() -> Result<Vec<String>> {
        let mut devices: Vec<String> = vec![];
        let client = Client::new_future().await?;
        for interface in client.devices() {
            match interface.device_type() {
                DeviceType::Ethernet => devices.push(interface.to_string()),
                _ => (),
            }
        }
        devices.sort();
        Ok(devices)
    }

    /// Check whether a new Network Mananger connection is created
    /// by the MapNMManger instance.
    fn is_created(&mut self, name: &str) -> bool {
        self.created_ifaces.contains(&name.to_string())
    }

    /// Get connection by name, if connection with the given name
    /// is not existed. It will be created.
    fn get_connection(&self, name: &str) {
        if self.map.contains_key(name) {}
    }

    /// Create a new Ethernet Network Manager connection with the
    /// connection name and device name.
    fn create_connection(conn_name: &str, device_name: Option<&str>) -> nm::SimpleConnection {
        let connection = SimpleConnection::new();
        let s_connection = SettingConnection::new();

        s_connection.set_type(Some(&SETTING_WIRED_SETTING_NAME));
        s_connection.set_id(Some(conn_name));
        s_connection.set_autoconnect(true);
        s_connection.set_interface_name(device_name);
        connection.add_setting(&s_connection);
        connection
    }

    /// Get the connection by given connection name, if the connection
    /// is not existed, it will be created according to the instance map
    /// attribute.
    pub async fn connection_by_name(&mut self, name: &str) -> Result<nm::RemoteConnection> {
        match self.map.contains_key(name) {
            true => {
                let client = Client::new_future().await?;
                let conn: nm::RemoteConnection;
                match client.connection_by_id(name) {
                    Some(connection) => conn = connection,
                    _ => {
                        let new_conn = MapNMManger::create_connection(
                            name,
                            self.map.get(name).map(|x| x.as_str()),
                        );
                        client.add_connection_future(&new_conn, true).await?;
                        // Add the new connection name to created_ifaces deque.
                        self.created_ifaces.push_back(name.to_string());
                        match client.connection_by_id(name) {
                            Some(connection) => conn = connection,
                            _ => bail!("Failed to get connection {}", name),
                        }
                    }
                }
                Ok(conn)
            }
            _ => bail!("Failed to get connection {}", name),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use glib::MainContext;

    #[test]
    fn test_map_networkcfg() {
        let ctx = MainContext::default();
        loop {
            match ctx.with_thread_default(|| {
                let mut mapnm_manger = ctx.block_on(MapNMManger::new_future(None)).unwrap();
                let conn = ctx
                    .block_on(mapnm_manger.connection_by_name("eth0"))
                    .unwrap();
                println!("{}", conn);
            }) {
                Ok(_) => break,
                _ => (),
            }
        }
    }
}
