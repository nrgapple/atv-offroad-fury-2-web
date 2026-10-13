use crate::*;

const MAGIC: &[u8; 4] = b"ATVM";
const HEADER: usize = 8;

/// Encodes only validated messages. Packet lengths include the eight-byte header.
pub fn encode(packet: &Packet) -> Result<Vec<u8>> {
    let mut w = Writer(Vec::with_capacity(2048));
    let kind = match packet {
        Packet::Join { .. } => 1,
        Packet::Welcome { .. } => 2,
        Packet::Lobby(_) => 3,
        Packet::Snapshot(_) => 4,
        Packet::Leave { .. } => 5,
        Packet::Ping { .. } => 6,
        Packet::Pong { .. } => 7,
        Packet::SetLobby { .. } => 8,
    };
    w.0.extend_from_slice(MAGIC);
    w.byte(VERSION);
    w.byte(kind);
    w.u16(0);
    match packet {
        Packet::Join { code, player } => {
            room(code)?;
            w.0.extend_from_slice(code);
            w.participant(player)?;
        }
        Packet::Welcome { peer, code, lobby } => {
            peer_id(*peer)?;
            if !lobby.players.iter().any(|p| p.id == *peer) {
                return Err(Error::InvalidValue);
            }
            room(code)?;
            w.byte(*peer);
            w.0.extend_from_slice(code);
            w.lobby(lobby)?;
        }
        Packet::Lobby(lobby) => w.lobby(lobby)?,
        Packet::SetLobby {
            epoch,
            map,
            started,
        } => {
            map_id(*map)?;
            w.u32(*epoch);
            w.u16(*map);
            w.byte(u8::from(*started));
        }
        Packet::Snapshot(s) => {
            s.validate()?;
            w.byte(s.peer);
            w.u32(s.sequence);
            w.u32(s.time_ms);
            w.u16(s.atv_model);
            w.u16(s.rider_model);
            w.transform(s.atv);
            w.floats(&s.velocity);
            w.floats(&s.angular_velocity);
            w.nodes(&s.atv_nodes);
            w.nodes(&s.rider_nodes);
            w.byte(s.rider_state as u8);
            w.transform(s.rider);
            w.floats(&s.rider_velocity);
            w.f32(s.rpm);
            w.f32(s.throttle);
            w.f32(s.speed);
            w.u32(s.crash_serial);
            w.f32(s.train_time.unwrap_or(-1.));
            w.byte(u8::from(s.train_running));
            w.byte(s.platform.as_ref().map_or(255, |p| p.car));
            if let Some(p) = &s.platform {
                w.transform(p.transform);
            }
        }
        Packet::Leave { peer } => {
            peer_id(*peer)?;
            w.byte(*peer);
        }
        Packet::Ping { nonce } | Packet::Pong { nonce } => w.u32(*nonce),
    }
    if w.0.len() > MAX_PACKET_BYTES {
        return Err(Error::InvalidLength);
    }
    let len = (w.0.len() - HEADER) as u16;
    w.0[6..8].copy_from_slice(&len.to_le_bytes());
    Ok(w.0)
}
/// Rejects oversized, truncated, ambiguous, malformed and trailing-byte packets
/// before allowing them into either the relay or a presentation buffer.
pub fn decode(bytes: &[u8]) -> Result<Packet> {
    if bytes.len() > MAX_PACKET_BYTES || bytes.len() < HEADER {
        return Err(Error::InvalidLength);
    }
    let mut r = Reader { bytes, offset: 0 };
    if r.take(4)? != MAGIC {
        return Err(Error::InvalidHeader);
    }
    if r.byte()? != VERSION {
        return Err(Error::UnsupportedVersion);
    }
    let kind = r.byte()?;
    if r.u16()? as usize != bytes.len() - HEADER {
        return Err(Error::InvalidLength);
    }
    let packet = match kind {
        1 => Packet::Join {
            code: r.room()?,
            player: r.participant()?,
        },
        2 => {
            let peer = r.peer()?;
            let code = r.room()?;
            let lobby = r.lobby()?;
            if !lobby.players.iter().any(|p| p.id == peer) {
                return Err(Error::InvalidValue);
            }
            Packet::Welcome { peer, code, lobby }
        }
        3 => Packet::Lobby(r.lobby()?),
        4 => {
            let s = Snapshot {
                peer: r.peer()?,
                sequence: r.u32()?,
                time_ms: r.u32()?,
                atv_model: r.u16()?,
                rider_model: r.u16()?,
                atv: r.transform()?,
                velocity: r.floats()?,
                angular_velocity: r.floats()?,
                atv_nodes: r.nodes()?,
                rider_nodes: r.nodes()?,
                rider_state: match r.byte()? {
                    0 => RiderState::Riding,
                    1 => RiderState::Airborne,
                    2 => RiderState::Wreck,
                    3 => RiderState::Recovering,
                    _ => return Err(Error::InvalidValue),
                },
                rider: r.transform()?,
                rider_velocity: r.floats()?,
                rpm: r.f32()?,
                throttle: r.f32()?,
                speed: r.f32()?,
                crash_serial: r.u32()?,
                train_time: {
                    let t = r.f32()?;
                    if t == -1. { None } else { Some(t) }
                },
                train_running: match r.byte()? {
                    0 => false,
                    1 => true,
                    _ => return Err(Error::InvalidValue),
                },
                platform: match r.byte()? {
                    255 => None,
                    car => Some(Box::new(crate::Platform {
                        car,
                        transform: r.transform()?,
                    })),
                },
            };
            s.validate()?;
            Packet::Snapshot(s)
        }
        5 => Packet::Leave { peer: r.peer()? },
        6 => Packet::Ping { nonce: r.u32()? },
        7 => Packet::Pong { nonce: r.u32()? },
        8 => {
            let epoch = r.u32()?;
            let map = r.u16()?;
            map_id(map)?;
            Packet::SetLobby {
                epoch,
                map,
                started: r.boolean()?,
            }
        }
        _ => return Err(Error::UnknownKind),
    };
    if r.offset != bytes.len() {
        return Err(Error::InvalidLength);
    }
    Ok(packet)
}
fn map_id(id: u16) -> Result<()> {
    if valid_map(id) {
        Ok(())
    } else {
        Err(Error::InvalidValue)
    }
}
fn room(code: &RoomCode) -> Result<()> {
    if valid_room_code(code) {
        Ok(())
    } else {
        Err(Error::InvalidRoomCode)
    }
}
fn peer_id(id: PeerId) -> Result<()> {
    if valid_peer(id) {
        Ok(())
    } else {
        Err(Error::InvalidValue)
    }
}
fn participant(p: &Participant) -> Result<()> {
    peer_id(p.id)?;
    if p.name.is_empty() || p.name.len() > MAX_NAME_BYTES || p.name.chars().any(char::is_control) {
        return Err(Error::InvalidValue);
    }
    Ok(())
}
fn lobby(l: &Lobby) -> Result<()> {
    peer_id(l.host)?;
    map_id(l.map)?;
    if l.players.is_empty() || l.players.len() > MAX_PLAYERS {
        return Err(Error::InvalidValue);
    }
    let mut mask = 0u8;
    for p in &l.players {
        participant(p)?;
        if mask & (1 << p.id) != 0 {
            return Err(Error::InvalidValue);
        }
        mask |= 1 << p.id;
    }
    if mask & (1 << l.host) == 0 {
        return Err(Error::InvalidValue);
    }
    Ok(())
}
struct Writer(Vec<u8>);
impl Writer {
    fn byte(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn floats(&mut self, v: &[f32]) {
        for v in v {
            self.f32(*v);
        }
    }
    fn transform(&mut self, t: Transform) {
        self.floats(&t.position);
        self.floats(&t.rotation);
        self.floats(&t.scale);
        self.floats(&t.shear);
    }
    fn nodes(&mut self, n: &[Transform]) {
        self.byte(n.len() as u8);
        for t in n {
            self.transform(*t);
        }
    }
    fn participant(&mut self, p: &Participant) -> Result<()> {
        participant(p)?;
        self.byte(p.id);
        self.byte(p.name.len() as u8);
        self.0.extend_from_slice(p.name.as_bytes());
        self.u16(p.atv_model);
        self.u16(p.rider_model);
        Ok(())
    }
    fn lobby(&mut self, l: &Lobby) -> Result<()> {
        lobby(l)?;
        self.byte(l.host);
        self.u32(l.epoch);
        self.u16(l.map);
        self.byte(u8::from(l.started));
        self.byte(l.players.len() as u8);
        for p in &l.players {
            self.participant(p)?;
        }
        Ok(())
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.offset.checked_add(n).ok_or(Error::InvalidLength)?;
        let slice = self.bytes.get(self.offset..end).ok_or(Error::Truncated)?;
        self.offset = end;
        Ok(slice)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn floats<const N: usize>(&mut self) -> Result<[f32; N]> {
        let mut out = [0.; N];
        for v in &mut out {
            *v = self.f32()?;
        }
        Ok(out)
    }
    fn boolean(&mut self) -> Result<bool> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::InvalidValue),
        }
    }
    fn peer(&mut self) -> Result<PeerId> {
        let p = self.byte()?;
        peer_id(p)?;
        Ok(p)
    }
    fn room(&mut self) -> Result<RoomCode> {
        let code = self.take(ROOM_CODE_BYTES)?.try_into().unwrap();
        room(&code)?;
        Ok(code)
    }
    fn transform(&mut self) -> Result<Transform> {
        let t = Transform {
            position: self.floats()?,
            rotation: self.floats()?,
            scale: self.floats()?,
            shear: self.floats()?,
        };
        t.validate()?;
        Ok(t)
    }
    fn nodes(&mut self) -> Result<Vec<Transform>> {
        let n = self.byte()? as usize;
        if n > MAX_NODES {
            return Err(Error::InvalidLength);
        }
        // Check all bytes before allocating any node storage.
        if n * 52 > self.bytes.len() - self.offset {
            return Err(Error::Truncated);
        }
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(self.transform()?);
        }
        Ok(out)
    }
    fn participant(&mut self) -> Result<Participant> {
        let id = self.peer()?;
        let n = self.byte()? as usize;
        if n > MAX_NAME_BYTES {
            return Err(Error::InvalidLength);
        }
        let name = std::str::from_utf8(self.take(n)?)
            .map_err(|_| Error::InvalidUtf8)?
            .to_owned();
        let p = Participant {
            id,
            name,
            atv_model: self.u16()?,
            rider_model: self.u16()?,
        };
        participant(&p)?;
        Ok(p)
    }
    fn lobby(&mut self) -> Result<Lobby> {
        let host = self.peer()?;
        let epoch = self.u32()?;
        let map = self.u16()?;
        let started = self.boolean()?;
        let n = self.byte()? as usize;
        if n > MAX_PLAYERS {
            return Err(Error::InvalidLength);
        }
        let mut players = Vec::with_capacity(n);
        for _ in 0..n {
            players.push(self.participant()?);
        }
        let l = Lobby {
            host,
            epoch,
            map,
            started,
            players,
        };
        lobby(&l)?;
        Ok(l)
    }
}
