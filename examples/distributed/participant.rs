use super::common::{Message};

use message_io::events::{EventQueue};
use message_io::network::{Network, NetEvent, Endpoint, Transport};

use std::net::{SocketAddr};
use std::collections::{HashMap};

pub struct Participant {
    network: Network,
    event_queue: EventQueue<NetEvent>,
    name: String,
    discovery_endpoint: Endpoint,
    public_addr: SocketAddr,
    known_participants: HashMap<String, Endpoint>, // Used only for free resources later
}

impl Participant {
    pub fn new(name: &str) -> Option<Participant> {
        let (mut network, event_queue) = Network::split();

        // A listener for any other participant that want to establish connection.
        // 'addr' contains the port that the OS gives for us when we put a 0.
        let listen_addr = "127.0.0.1:0";
        let listen_addr = match network.listen(Transport::Udp, listen_addr) {
            Ok((_, addr)) => addr,
            Err(_) => {
                println!("Can not listen on {}", listen_addr);
                return None
            }
        };

        let discovery_addr = "127.0.0.1:5000"; // Connection to the discovery server.
        match network.connect(Transport::FramedTcp, discovery_addr) {
            Ok((endpoint, _)) => Some(Participant {
                event_queue,
                network,
                name: name.to_string(),
                discovery_endpoint: endpoint,
                public_addr: listen_addr,
                known_participants: HashMap::new(),
            }),
            Err(_) => {
                println!("Can not connect to the discovery server at {}", discovery_addr);
                return None
            }
        }
    }

    pub fn run(mut self) {
        // Register this participant into the discovery server
        let message = Message::RegisterParticipant(self.name.clone(), self.public_addr);
        let output_data = bincode::serialize(&message).unwrap();
        self.network.send(self.discovery_endpoint, &output_data);

        loop {
            match self.event_queue.receive() {
                // Waiting events
                NetEvent::Message(_, input_data) => {
                    let message: Message = bincode::deserialize(&input_data).unwrap();
                    match message {
                        Message::ParticipantList(participants) => {
                            println!(
                                "Participant list received ({} participants)",
                                participants.len()
                            );
                            for (name, addr) in participants {
                                self.discovered_participant(
                                    &name,
                                    addr,
                                    "I see you in the participant list",
                                );
                            }
                        }
                        Message::ParticipantNotificationAdded(name, addr) => {
                            println!("New participant '{}' in the network", name);
                            self.discovered_participant(&name, addr, "welcome to the network!");
                        }
                        Message::ParticipantNotificationRemoved(name) => {
                            println!("Removed participant '{}' from the network", name);

                            // Free related network resources to the endpoint.
                            // It is only necessary because the connections among participants
                            // are done by UDP,
                            // UDP is not connection-oriented protocol, and the
                            // AddedEndpoint/RemoveEndpoint events are not generated by UDP.
                            if let Some(endpoint) = self.known_participants.remove(&name) {
                                self.network.remove(endpoint.resource_id());
                            }
                        }
                        Message::Gretings(name, gretings) => {
                            println!("'{}' says: {}", name, gretings);
                        }
                        _ => unreachable!(),
                    }
                }
                NetEvent::Connected(_) => (),
                NetEvent::Disconnected(endpoint) => {
                    if endpoint == self.discovery_endpoint {
                        return println!("Discovery server disconnected, closing")
                    }
                }
            }
        }
    }

    fn discovered_participant(&mut self, name: &str, addr: SocketAddr, message: &str) {
        if let Ok((endpoint, _)) = self.network.connect(Transport::Udp, addr) {
            let gretings = format!("Hi '{}', {}", name, message);
            let message = Message::Gretings(self.name.clone(), gretings);
            let output_data = bincode::serialize(&message).unwrap();
            self.network.send(endpoint, &output_data);
            self.known_participants.insert(name.to_string(), endpoint);
        }
    }
}
