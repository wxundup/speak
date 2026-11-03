use super::frame::MAX_VOICE_CHAT_PACKET_SIZE;

/// The server config values the mix reads
/// (`SERVER_CONFIG`: max_voice_distance 48, whisper_distance 24,
/// broadcast_range −1, spectator flags false).
#[derive(Clone, Copy, Debug)]
pub struct MixConfig {
    /// `max_voice_distance`, default 48 blocks.
    pub max_voice_distance: f64,
    /// `whisper_distance`, default 24 blocks.
    pub whisper_distance: f64,
    /// `broadcast_range`; negative means `max_voice_distance + 1`.
    pub broadcast_range: f64,
    /// `spectator_interaction`: a spectator may talk to other players.
    pub spectator_interaction: bool,
    pub spectator_player_possession: bool,
}

impl Default for MixConfig {
    fn default() -> Self {
        Self {
            max_voice_distance: 48.0,
            whisper_distance: 24.0,
            broadcast_range: -1.0,
            spectator_interaction: false,
            spectator_player_possession: false,
        }
    }
}

/// One player's state as the mix reads it (`NativeServerPlayer` fields:
/// uuid, name, dimension, position, game mode; the group lives in the
/// group manager).
#[derive(Clone, Debug, PartialEq)]
pub struct MixPlayer {
    pub uuid: u128,
    pub name: String,
    /// `minecraft:overworld` and friends.
    pub dimension: String,
    pub position: [f64; 3],
    /// 0 survival, 1 creative, 2 adventure, 3 spectator.
    pub game_mode: i32,
    /// Whether the player's voice chat is disabled (their own choice or a
    /// forced state): a disabled player neither sends nor receives
    /// (`PlayerState.isDisabledOrForceDisabled` guards both ends).
    pub disabled: bool,
    /// Whether the player is force-muted: their mic packets are dropped
    /// before the mix (`Server.onMicPacket`).
    pub force_muted: bool,
}

impl MixPlayer {
    /// A spectator (`ServerPlayer.isSpectator`).
    pub fn is_spectator(&self) -> bool {
        self.game_mode == 3
    }
}

/// The mix for one mic packet: who should hear it. The caller hands the
/// sender, the packet's whisper flag, the players online (the sender
/// included), and the group routing predicates; the mix answers with the
/// receivers and the distance each packet should carry.
pub struct Mix<'a> {
    pub config: MixConfig,
    /// The players online (`server.getPlayerList().getPlayers()`), the
    /// sender included.
    pub players: &'a [MixPlayer],
    /// Whether two players share a group (`groups::same_group`).
    pub same_group: &'a dyn Fn(u128, u128) -> bool,
    /// Whether the sender's group is open (`groups::sender_open`).
    pub sender_open: &'a dyn Fn(u128) -> bool,
    /// Whether the sender's group is isolated (`groups::sender_isolated`).
    pub sender_isolated: &'a dyn Fn(u128) -> bool,
    /// Whether the receiver's group is isolated
    /// (`groups::receiver_isolated`).
    pub receiver_isolated: &'a dyn Fn(u128) -> bool,
}

/// A receiver the mix picked: the player and the packet's shape.
#[derive(Clone, Debug, PartialEq)]
pub enum Receiver {
    /// A group member hears the group packet
    /// (`GroupSoundPacket`, both ids the sender's).
    Group { player: u128 },
    /// A nearby player hears the player packet
    /// (`PlayerSoundPacket`, with the mix's distance).
    Proximity { player: u128, distance: f32 },
    /// A spectator's audience hears the locational packet
    /// (`LocationSoundPacket`, at the eye position). The position comes
    /// from the sender's state, which the caller has.
    Location { player: u128 },
}

impl Mix<'_> {
    /// The voice distance for this packet: the whisper distance when the
    /// sender whispers, else the max voice distance
    /// (`processProximityPacket`'s `distance`).
    pub fn voice_distance(&self, whisper: bool) -> f64 {
        if whisper {
            self.config.whisper_distance
        } else {
            self.config.max_voice_distance
        }
    }

    /// The search radius (`Server.getBroadcastRange`): the config's
    /// broadcast range when non-negative, else `distance + 1`; never under
    /// the voice distance itself.
    pub fn broadcast_range(&self, distance: f64) -> f64 {
        let mut range = self.config.broadcast_range;
        if range < 0.0 {
            range = self.config.max_voice_distance + 1.0;
        }
        range.max(distance)
    }

    /// The Euclidean distance between two players' positions.
    fn apart(a: [f64; 3], b: [f64; 3]) -> f64 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }

    /// The receivers for one mic packet (`onMicPacket` →
    /// `processMicPacket`): None when the sender may not speak at all.
    pub fn receivers(&self, sender: u128, whisper: bool) -> Option<Vec<Receiver>> {
        let Some(sender_state) = self.players.iter().find(|p| p.uuid == sender) else {
            return None;
        };
        // A force-muted or disabled sender's packets are dropped before the
        // mix (Server.onMicPacket's state checks).
        if sender_state.force_muted || sender_state.disabled {
            return None;
        }
        // A group member reaches their group first (`processGroupPacket`).
        let mut out = Vec::new();
        let in_group = (self.same_group)(sender, sender) // placeholder, never true
            || self.player_group_members(sender, sender_state).is_some();
        let _ = in_group;
        if let Some(members) = self.player_group_members(sender, sender_state) {
            for member in members {
                if member == sender {
                    continue;
                }
                if self.player_by(member).is_some_and(|p| p.disabled) {
                    continue;
                }
                out.push(Receiver::Group { player: member });
            }
        }
        let isolated_sender = (self.sender_isolated)(sender);
        if !isolated_sender {
            // The proximity pass: the open-group and groupless senders both
            // reach nearby players; their own group's members are excluded
            // (they already got the group packet), and isolated receivers
            // hear no proximity (`broadcast`'s two guards).
            let distance = self.voice_distance(whisper);
            let range = self.broadcast_range(distance);
            let spectator_voice = sender_state.is_spectator();
            for receiver in self.players {
                if receiver.uuid == sender {
                    continue;
                }
                if receiver.disabled {
                    continue;
                }
                if (self.same_group)(sender, receiver.uuid) {
                    continue;
                }
                if (self.receiver_isolated)(receiver.uuid) {
                    continue;
                }
                // Spectators without the interaction config are silent to
                // proximity (`spectatorInteraction` false: the packet is
                // made but no receiver hears it — the loop keeps its
                // distance, with an empty broadcast list).
                if spectator_voice && !self.config.spectator_interaction {
                    continue;
                }
                let apart = Self::apart(receiver.position, sender_state.position);
                // A different dimension never hears (`p.level() != level`
                // in getPlayersInRange); the mix reads the dimension as the
                // level's name.
                if receiver.dimension != sender_state.dimension {
                    continue;
                }
                if apart <= range {
                    // A spectator with the interaction config sends a
                    // locational packet instead of a player packet.
                    let receiver_kind = if spectator_voice {
                        Receiver::Location { player: receiver.uuid }
                    } else {
                        Receiver::Proximity {
                            player: receiver.uuid,
                            distance: distance as f32,
                        }
                    };
                    out.push(receiver_kind);
                }
            }
        }
        Some(out)
    }

    /// The sender's group's members, from the players list (the group
    /// manager holds the membership; the mix needs the states to skip
    /// disabled members, so it filters over `players`).
    fn player_group_members(&self, sender: u128, _sender_state: &MixPlayer) -> Option<Vec<u128>> {
        // The caller's `same_group` predicate answers membership per pair;
        // the members are the players that share the sender's group.
        if !(self.same_group)(sender, sender) && self.groupless(sender) {
            return None;
        }
        let members: Vec<u128> = self
            .players
            .iter()
            .filter(|p| (self.same_group)(sender, p.uuid))
            .map(|p| p.uuid)
            .collect();
        Some(members)
    }

    /// Whether the sender is in no group at all (the predicate pair says
    /// so: nothing shares their group and they share nobody's).
    fn groupless(&self, sender: u128) -> bool {
        // A groupless sender shares no group with anyone, including
        // themselves. The `same_group(a, a)` call above is the tell: the
        // group manager answers false when the player is in no group.
        !(self.same_group)(sender, sender)
    }

    fn player_by(&self, uuid: u128) -> Option<&MixPlayer> {
        self.players.iter().find(|p| p.uuid == uuid)
    }
}

pub fn frame_too_large(frame: &[u8]) -> bool {
    frame.len() > MAX_VOICE_CHAT_PACKET_SIZE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(uuid: u128, x: f64, dimension: &str) -> MixPlayer {
        MixPlayer {
            uuid,
            name: format!("P{uuid}"),
            dimension: dimension.into(),
            position: [x, 64.0, 0.0],
            game_mode: 0,
            disabled: false,
            force_muted: false,
        }
    }

    /// A group table: 10+11 share "open" (Open), 20+21 share "iso"
    /// (Isolated), 30+31 share "norm" (Normal), 40 is groupless.
    fn mix(players: Vec<MixPlayer>) -> Mix<'static> {
        mix_with(MixConfig::default(), players)
    }

    /// The same table under a custom config.
    fn mix_with(config: MixConfig, players: Vec<MixPlayer>) -> Mix<'static> {
        fn same_group(a: u128, b: u128) -> bool {
            let group_of = |p: u128| match p {
                10 | 11 => Some(1),
                20 | 21 => Some(2),
                30 | 31 => Some(3),
                _ => None,
            };
            match (group_of(a), group_of(b)) {
                (Some(x), Some(y)) => x == y,
                _ => false,
            }
        }
        fn sender_open(p: u128) -> bool {
            p == 10 || p == 11
        }
        fn sender_isolated(p: u128) -> bool {
            p == 20 || p == 21
        }
        fn receiver_isolated(p: u128) -> bool {
            p == 20 || p == 21
        }
        let players = Box::leak(players.into_boxed_slice());
        Mix {
            config,
            players,
            same_group: &same_group,
            sender_open: &sender_open,
            sender_isolated: &sender_isolated,
            receiver_isolated: &receiver_isolated,
        }
    }

    #[test]
    fn a_groupless_sender_reaches_the_nearby_only() {
        let mix = mix(vec![
            player(40, 0.0, "o"),
            player(41, 10.0, "o"),
            player(42, 500.0, "o"),
        ]);
        let receivers = mix.receivers(40, false).unwrap();
        assert_eq!(
            receivers,
            vec![Receiver::Proximity { player: 41, distance: 48.0 }]
        );
    }

    #[test]
    fn the_open_group_reaches_members_and_proximity_but_not_isolated() {
        let mix = mix(vec![
            player(10, 0.0, "o"),   // open group
            player(11, 5.0, "o"),   // open group, near
            player(40, 10.0, "o"),  // groupless, near
            player(21, 12.0, "o"),  // isolated group, near
            player(42, 500.0, "o"), // groupless, far
        ]);
        let receivers = mix.receivers(10, false).unwrap();
        assert!(receivers.contains(&Receiver::Group { player: 11 }), "the member");
        assert!(
            receivers.contains(&Receiver::Proximity { player: 40, distance: 48.0 }),
            "an open group bleeds to proximity"
        );
        assert!(
            !receivers.iter().any(|r| matches!(r, Receiver::Group { player: 10 })),
            "the sender is never their own receiver"
        );
        assert!(
            !receivers.iter().any(|r| matches!(r, Receiver::Proximity { player: 21, .. })),
            "an isolated receiver hears no proximity"
        );
        assert!(!receivers.iter().any(|r| matches!(r, Receiver::Proximity { player: 42, .. })));
    }

    #[test]
    fn the_isolated_sender_reaches_only_members() {
        let mix = mix(vec![
            player(20, 0.0, "o"),
            player(21, 500.0, "o"), // far, but a member
            player(40, 3.0, "o"),   // near, but a stranger
        ]);
        let receivers = mix.receivers(20, false).unwrap();
        assert_eq!(receivers, vec![Receiver::Group { player: 21 }]);
    }

    #[test]
    fn the_normal_group_reaches_members_and_still_bleeds() {
        let mix = mix(vec![
            player(30, 0.0, "o"),  // normal group
            player(31, 5.0, "o"),  // normal group, near
            player(40, 8.0, "o"),  // groupless, near
        ]);
        let receivers = mix.receivers(30, false).unwrap();
        assert_eq!(
            receivers,
            vec![
                Receiver::Group { player: 31 },
                Receiver::Proximity { player: 40, distance: 48.0 },
            ]
        );
    }

    #[test]
    fn the_server_sends_by_broadcast_range_and_the_packet_carries_the_distance() {
        let mix = mix(vec![
            player(40, 0.0, "o"),
            player(41, 20.0, "o"),  // within whisper (24), within voice
            player(42, 30.0, "o"),  // past whisper (24), within voice (48)
        ]);
        let loud = mix.receivers(40, false).unwrap();
        assert!(loud.iter().any(|r| matches!(r, Receiver::Proximity { player: 41, distance: 48.0 })));
        assert!(loud.iter().any(|r| matches!(r, Receiver::Proximity { player: 42, distance: 48.0 })));
        let whisper = mix.receivers(40, true).unwrap();
        assert!(whisper.iter().any(|r| matches!(r, Receiver::Proximity { player: 41, distance: 24.0 })));
        assert!(whisper.iter().any(|r| matches!(r, Receiver::Proximity { player: 42, distance: 24.0 })));
    }

    #[test]
    fn another_dimension_never_hears() {
        let mix = mix(vec![
            player(40, 0.0, "overworld"),
            player(41, 1.0, "the_nether"),
        ]);
        let receivers = mix.receivers(40, false).unwrap();
        assert!(receivers.is_empty());
    }

    #[test]
    fn a_disabled_receiver_hears_nothing_and_a_disabled_sender_sends_nothing() {
        let mut players = vec![player(40, 0.0, "o"), player(41, 5.0, "o")];
        players[1].disabled = true;
        let m = mix(players);
        assert!(m.receivers(40, false).unwrap().is_empty());
        let mut players = vec![player(40, 0.0, "o"), player(41, 5.0, "o")];
        players[0].disabled = true;
        let m = mix(players);
        assert_eq!(m.receivers(40, false), None, "a disabled sender is dropped");
    }

    #[test]
    fn a_force_muted_sender_is_dropped_before_the_mix() {
        let mut players = vec![player(40, 0.0, "o"), player(41, 5.0, "o")];
        players[0].force_muted = true;
        let m = mix(players);
        assert_eq!(m.receivers(40, false), None);
    }

    #[test]
    fn spectators_are_silent_unless_the_interaction_config_allows() {
        let mut players = vec![player(40, 0.0, "o"), player(41, 5.0, "o")];
        players[0].game_mode = 3;
        let m = mix(players);
        assert!(m.receivers(40, false).unwrap().is_empty(), "default config");
        let mut players = vec![player(40, 0.0, "o"), player(41, 5.0, "o")];
        players[0].game_mode = 3;
        let mut config = MixConfig::default();
        config.spectator_interaction = true;
        let m = mix_with(config, players);
        let receivers = m.receivers(40, false).unwrap();
        assert_eq!(
            receivers,
            vec![Receiver::Location { player: 41 }],
            "a spectator's voice is locational"
        );
    }

    #[test]
    fn the_broadcast_range_never_dips_under_the_voice_distance() {
        let m = mix(vec![player(40, 0.0, "o")]);
        assert_eq!(m.broadcast_range(48.0), 49.0, "default: distance + 1");
        let mut config = MixConfig::default();
        config.broadcast_range = 100.0;
        let m = mix_with(config, vec![player(40, 0.0, "o")]);
        assert_eq!(m.broadcast_range(48.0), 100.0);
        let mut config = MixConfig::default();
        config.broadcast_range = 10.0;
        let m = mix_with(config, vec![player(40, 0.0, "o")]);
        assert_eq!(m.broadcast_range(48.0), 48.0, "never under the distance");
    }

    #[test]
    fn a_too_large_frame_is_dropped() {
        assert!(!frame_too_large(&vec![0u8; 2048]));
        assert!(frame_too_large(&vec![0u8; 2049]));
    }
}
