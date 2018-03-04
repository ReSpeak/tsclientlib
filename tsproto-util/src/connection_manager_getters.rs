#[derive(Template)]
#[TemplatePath = "src/ConnectionManagerGetters.tt"]
#[derive(Debug)]
pub struct ConnectionManagerGetters {
    getters: Vec<Getter>,
}

#[derive(Debug)]
struct Getter {
    name: &'static str,
    args: &'static str,
    return_type: &'static str,
    has: &'static str,
    get: &'static str,
    get_mut: &'static str,
    add: Option<&'static str>,
    remove: Option<&'static str>,
}

impl Default for ConnectionManagerGetters {
    fn default() -> Self {
        let mut res = Self { getters: Vec::new() };

        res.getters.push(Getter {
            name: "server",
            args: "con: ConnectionId",
            return_type: "structs::Server",
            has: "r.connections.contains_key(&con)",
            get: "&r.connections[&con].server",
            get_mut: "&mut r.connections.get_mut(&con).unwrap().server",
            add: None,
            remove: None,
        });

        res.getters.push(Getter {
            name: "optional_server_data",
            args: "con: ConnectionId",
            return_type: "structs::OptionalServerData",
            has: "r.connections.get(&con)
                .map(|c| c.server.optional_data.is_some())
                .unwrap_or(false)",
            get: "r.connections[&con].server.optional_data.as_ref().unwrap()",
            get_mut: "r.connections.get_mut(&con).unwrap()
                .server.optional_data.as_mut().unwrap()",
            add: None, // TODO
            remove: None,
        });

        res.getters.push(Getter {
            name: "connection_server_data",
            args: "con: ConnectionId",
            return_type: "structs::ConnectionServerData",
            has: "r.connections.get(&con)
                .map(|c| c.server.connection_data.is_some())
                .unwrap_or(false)",
            get: "r.connections[&con].server.connection_data.as_ref().unwrap()",
            get_mut: "r.connections.get_mut(&con).unwrap()
                .server.connection_data.as_mut().unwrap()",
            add: None, // TODO
            remove: None,
        });

        res.getters.push(Getter {
            name: "client",
            args: "con: ConnectionId, client: ClientId",
            return_type: "structs::Client",
            has: "r.connections.get(&con)
                .map(|c| c.server.clients.contains_key(&client))
                .unwrap_or(false)",
            get: "&r.connections[&con].server.clients[&client]",
            get_mut: "r.connections.get_mut(&con).unwrap()
                .server.clients.get_mut(&client).unwrap()",
            add: None, // TODO
            remove: None, // TODO
        });

        res.getters.push(Getter {
            name: "optional_client_data",
            args: "con: ConnectionId, client: ClientId",
            return_type: "structs::OptionalClientData",
            has: "r.connections.get(&con)
                .and_then(|c| c.server.clients.get(&client))
                .map(|c| c.optional_data.is_some())
                .unwrap_or(false)",
            get: "r.connections[&con].server.clients[&client]
                .optional_data.as_ref().unwrap()",
            get_mut: "r.connections.get_mut(&con).unwrap()
                .server.clients.get_mut(&client).unwrap()
                .optional_data.as_mut().unwrap()",
            add: None, // TODO
            remove: None,
        });

        res.getters.push(Getter {
            name: "connection_client_data",
            args: "con: ConnectionId, client: ClientId",
            return_type: "structs::ConnectionClientData",
            has: "r.connections.get(&con)
                .and_then(|c| c.server.clients.get(&client))
                .map(|c| c.connection_data.is_some())
                .unwrap_or(false)",
            get: "r.connections[&con].server.clients[&client]
                .connection_data.as_ref().unwrap()",
            get_mut: "r.connections.get_mut(&con).unwrap()
                .server.clients.get_mut(&client).unwrap()
                .connection_data.as_mut().unwrap()",
            add: None, // TODO
            remove: None,
        });

        res.getters.push(Getter {
            name: "channel",
            args: "con: ConnectionId, channel: ChannelId",
            return_type: "structs::Channel",
            has: "r.connections.get(&con)
                .map(|c| c.server.channels.contains_key(&channel))
                .unwrap_or(false)",
            get: "&r.connections[&con].server.channels[&channel]",
            get_mut: "r.connections.get_mut(&con).unwrap()
                .server.channels.get_mut(&channel).unwrap()",
            add: None, // TODO
            remove: None, // TODO
        });

        res.getters.push(Getter {
            name: "optional_channel_data",
            args: "con: ConnectionId, channel: ChannelId",
            return_type: "structs::OptionalChannelData",
            has: "r.connections.get(&con)
                .and_then(|c| c.server.channels.get(&channel))
                .map(|c| c.optional_data.is_some())
                .unwrap_or(false)",
            get: "r.connections[&con].server.channels[&channel]
                .optional_data.as_ref().unwrap()",
            get_mut: "r.connections.get_mut(&con).unwrap()
                .server.channels.get_mut(&channel).unwrap()
                .optional_data.as_mut().unwrap()",
            add: None, // TODO
            remove: None,
        });

        res.getters.push(Getter {
            name: "server_group",
            args: "con: ConnectionId, group: ServerGroupId",
            return_type: "structs::ServerGroup",
            has: "r.connections.get(&con)
                .map(|c| c.server.groups.contains_key(&group))
                .unwrap_or(false)",
            get: "&r.connections[&con].server.groups[&group]",
            get_mut: "r.connections.get_mut(&con).unwrap()
                .server.groups.get_mut(&group).unwrap()",
            add: None, // TODO
            remove: None, // TODO
        });

        res
    }
}
