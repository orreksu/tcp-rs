
// protocol.rs

use std::mem::size_of;
use std::convert::TryInto;

pub const TIMEOUT_SEC: u64   = 0;
pub const TIMEOUT_NSEC: u32  = 500_000_000;
pub const WINDOW_SIZE: u32 = 15;

const MSG_SIZE: usize        = 1500;
const IP_HEADER_SIZE: usize  = 20;
const UDP_HEADER_SIZE: usize = 8;

const PACKET_SIZE: usize = MSG_SIZE - IP_HEADER_SIZE - UDP_HEADER_SIZE;
const HEADER_SIZE: usize = size_of::<Header>();
const DATA_SIZE: usize   = PACKET_SIZE - HEADER_SIZE;

type Result<T> = std::result::Result<T, ProtocolError>;

pub struct ProtocolError(String);

impl ProtocolError{
    pub fn val(self) -> String {
        self.0
    }
}

impl From<std::array::TryFromSliceError> for ProtocolError {
    fn from(err: std::array::TryFromSliceError) -> ProtocolError {
        ProtocolError(err.to_string())
    }
}


// Note: len is the length of the packet includes the header
#[derive(Debug)]
pub struct Header {
    cod: u8,
    seq: [u8; 4],
    len: [u8; 4]
}

#[derive(Debug)]
pub enum Packet {
    Dat(Header, Vec<u8>),
    Ack(Header),
    Fin(Header)
}

enum PacketCode {
    Dat = 0,
    Ack = 1,
    Fin = 255
}


impl Header {

    const SIZE: u32 = HEADER_SIZE as u32;

    // Create new Header with given PacketCode, sequence number, and length
    fn new(cod: PacketCode, seq: u32, len: u32) -> Header {
        Header { 
            cod: cod as u8, 
            seq: seq.to_be_bytes(), 
            len: len.to_be_bytes() 
        }
    }

    // Create new Header for a Packet::Dat with given sequence number, and length
    fn new_dat(seq: u32, len: u32) -> Header {
        Header::new(PacketCode::Dat, seq, len)
    }

    // Create new Header for a Packet::Ack with given sequence number
    fn new_ack(seq: u32) -> Header {
        Header::new(PacketCode::Ack, seq, Header::SIZE)
    }

    // Create new Header for a Packet::Fin with given sequence number
    fn new_fin(seq: u32) -> Header {
        Header::new(PacketCode::Fin, seq, Header::SIZE)
    }

    // Parses given bytes as Header
    fn from_bytes(bytes: [u8; HEADER_SIZE]) -> Result<Header> {

        let cod = bytes[0];
        let seq = u32::from_be_bytes(bytes[1..5].try_into()?);
        let len = u32::from_be_bytes(bytes[5..HEADER_SIZE].try_into()?);

        match cod {
            cod if (cod == PacketCode::Dat as u8) => Ok(Header::new_dat(seq, len)),
            cod if (cod == PacketCode::Ack as u8) => Ok(Header::new_ack(seq)),
            cod if (cod == PacketCode::Fin as u8) => Ok(Header::new_fin(seq)),
            wrong_code => Err(ProtocolError(format!("Unknown PacketCode {}", wrong_code)))
        }
    }

    // Return the PacketCode of the header
    fn packet_code(&self) -> Result<PacketCode> {
        match self.cod {
            cod if (cod == PacketCode::Dat as u8) => Ok(PacketCode::Dat),
            cod if (cod == PacketCode::Ack as u8) => Ok(PacketCode::Ack),
            cod if (cod == PacketCode::Fin as u8) => Ok(PacketCode::Fin),
            wrong_code => Err(ProtocolError(format!("Unknown PacketCode {}", wrong_code)))
        }
    }

    // Return empty bytes array of size to read header from bite stream
    fn empty_bytes() -> [u8; HEADER_SIZE] {
        [0; HEADER_SIZE]
    }

    // Return the seq number in header as u32 integer
    fn seq(&self) -> u32 {
        u32::from_be_bytes(self.seq)
    }

    // Return the length of the packet in header as u32 integer
    fn len(&self) -> u32 {
        u32::from_be_bytes(self.len)
    }

    // Bytes representation of the header
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = [self.cod].to_vec();
        bytes.extend(self.seq.to_vec());
        bytes.extend(self.len.to_vec());
        bytes
    }  
}


impl Packet {

    pub const DATA_SIZE: usize = DATA_SIZE;

    // Create new Packet::Dat with given sequence number, and data
    pub fn new_dat_packet(seq: u32, data: Vec<u8>) -> Packet {
        let len = data.len() as u32;
        let head = Header::new_dat(seq, len + Header::SIZE);
        Packet::Dat(head, data)
    }

    // Create new Packet::Ack with given sequence number
    pub fn new_ack_packet(seq: u32) -> Packet {
        Packet::Ack(Header::new_ack(seq))
    }

    // Create new Packet::Fin with given sequence number
    pub fn new_fin_packet(seq: u32) -> Packet {
        Packet::Fin(Header::new_fin(seq))
    }

    // Create new Packet from given head bytes
    // Note: Packet::Dat in this case is initialized with empty data
    pub fn from_header_bytes(head_bytes: [u8; HEADER_SIZE]) -> Result<Packet> {
        let head = Header::from_bytes(head_bytes)?;
        match head.packet_code()? {
            PacketCode::Dat => Ok(Packet::Dat(head, Vec::new())),
            PacketCode::Ack => Ok(Packet::Ack(head)),
            PacketCode::Fin => Ok(Packet::Fin(head))
        }
    }

    // Set data if given Packet::Dat, otherwise return Error
    pub fn clone_with_data(self, packet_bytes: &mut Vec<u8>) -> Result<Packet> {
        match self {
            Packet::Dat(head, _) => {
                let data: Vec<_> = packet_bytes.drain(HEADER_SIZE..).collect();
                Ok(Packet::Dat(head, data))
            },
            _else => Err(ProtocolError("Trying to set data to non Packet::Dat".into()))?
        }
    }

    // Return the length of the packet in header as u32 integer
    pub fn len(&self) -> usize {
        match self {
            Packet::Dat(head, _) => head.len() as usize,
            Packet::Ack(head)    => head.len() as usize,
            Packet::Fin(head)    => head.len() as usize,
        }
    }

    // Return the seq number in header as u32 integer
    pub fn seq(&self) -> u32 {
        match self {
            Packet::Dat(head, _) => head.seq(),
            Packet::Ack(head)    => head.seq(),
            Packet::Fin(head)    => head.seq(),
        }
    }

    // Return empty bytes array of size to read header from bite stream
    pub fn empty_header_bytes() -> [u8; HEADER_SIZE] {
        Header::empty_bytes()
    }

    // Is given size equal to header size?
    pub fn is_header_size(size: usize) -> bool {
        size == HEADER_SIZE
    }

    // Return data of the data packet, otherwise return error
    pub fn data(&self) -> Result<Vec<u8>> {
        match self {
            Packet::Dat(_, data) => Ok(data.clone()),
            _else => Err(ProtocolError("Trying to get data from non Packet::Dat".into()))?
        }
    }

    // Bytes representation of the packet
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Packet::Dat(head, data) => {
                let mut bytes = head.to_bytes();
                bytes.extend(data);
                bytes
            },
            Packet::Ack(head)    => head.to_bytes(),
            Packet::Fin(head)    => head.to_bytes(),
        }
    }
}
