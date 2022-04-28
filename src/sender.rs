
// sender.rs

use std::env;
use std::net::{UdpSocket, SocketAddr};
use std::io::{self, Read};
use std::time::Duration;

pub mod protocol;
use protocol::{Packet, WINDOW_SIZE};

type Result<T> = std::result::Result<T, SenderError>;
type DataBlock = Vec<u8>;
type Seq = u32;
type IsEof = bool;

type Window = std::collections::VecDeque<Packet>;


// Custom error for Sender
// implements AddrParseError, io::Error
#[derive(Debug, Clone)]
struct SenderError(String);

impl From<std::net::AddrParseError> for SenderError {
    fn from(_: std::net::AddrParseError) -> SenderError {
        SenderError("Failed to parse socket address".into())
    }
}

impl From<std::io::Error> for SenderError {
    fn from(err: std::io::Error) -> SenderError {
        SenderError(err.to_string())
    }
}

impl From<protocol::ProtocolError> for SenderError {
    fn from(err: protocol::ProtocolError) -> SenderError {
        SenderError(err.val())
    }
}


// INITIALIZE ========

// Parse arguments to find address string,
// ensure that only one argument is given,
// create a socket address out of the string
fn remote_addr() -> Result<SocketAddr> {
    use std::str::FromStr;
    let remote_addr = env::args().nth(1).ok_or(SenderError("Wrong number of arguments".into()))?;
    eprintln!("SENDER: parsed remote addr: {}", remote_addr);
    Ok(SocketAddr::from_str(&remote_addr)?)
}


// Craete new socket with defualt timeout from protocol
// for both read and write
fn open_socket() -> Result<UdpSocket> {

    let timeout = Some(Duration::new(protocol::TIMEOUT_SEC, protocol::TIMEOUT_NSEC));
    eprintln!("SENDER: set timeout: sec: {}, nsec: {}", protocol::TIMEOUT_SEC, protocol::TIMEOUT_NSEC);

    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(timeout)?;
    socket.set_write_timeout(timeout)?;
    // socket.connect()?;

    eprintln!("SENDER: opened socket: {}", socket.local_addr()?);

    Ok(socket)
}


// Parse argument to find address, 
// initialize UDP socket with found address
fn init_connection() -> Result<(Seq, SocketAddr, UdpSocket)> {

    let seq = 0;
    eprintln!("SENDER: initialized seq: {}", seq);

    let socket = open_socket()?;
    let remote_addr = remote_addr()?;

    Ok((seq, remote_addr, socket))
}


// STDIN ========

// Read one data block of DATA_SIZE or less,
// if less, read till EOF and return true for `is_eof`,
// otherwise return false for `is_eof`
fn read_data_block() -> Result<(usize, DataBlock, IsEof)> {
    let mut data = Vec::new();
    let size = io::stdin().take(Packet::DATA_SIZE as u64).read_to_end(&mut data)?;
    let is_eof = size < Packet::DATA_SIZE;

    Ok((size, data, is_eof))
}


// SEND PACKETS ========

fn send_packet(packet: &Packet, remote_addr: SocketAddr, socket: &UdpSocket) -> Result<usize> {
    let packet_bytes = &packet.to_bytes()[..];
    let packet_size = packet_bytes.len();

    match socket.send_to(packet_bytes, remote_addr)? {
        send_size if send_size < packet_size => Err(SenderError("Did not send packet fully".into())),
        send_size if send_size > packet_size => Err(SenderError("Send bigger packet than needed".into())),
        send_size => Ok(send_size)
    }
}

fn send_ack_packet(seq: Seq, remote_addr: SocketAddr, socket: &UdpSocket) -> Result<()> {
    let packet = Packet::new_ack_packet(seq);
    send_packet(&packet, remote_addr, socket)?;
    eprintln!("SENDER: send ack packet");
    Ok(())
}

fn send_fin_packet(seq: Seq, remote_addr: SocketAddr, socket: &UdpSocket) -> Result<()> {
    let packet = Packet::new_fin_packet(seq);
    send_packet(&packet, remote_addr, socket)?;
    eprintln!("SENDER: send fin packet");
    Ok(())
}


// READ PACKETS ========

fn read_packet(socket: &UdpSocket) -> Result<Packet> {
    let mut packet_bytes = Packet::empty_header_bytes();
    let packet_size = socket.recv(&mut packet_bytes)?;

    eprintln!("SENDER: read packet of size: {}", packet_size);

    if Packet::is_header_size(packet_size) {
        Ok(Packet::from_header_bytes(packet_bytes)?)
    } else {
        Err(SenderError("read header size != header size".into()))
    }
}


fn receive_ack_packet(current_seq: Seq, socket: &UdpSocket) -> Result<()> {
    eprintln!("SENDER: waiting for packet...");
    match read_packet(socket)? {
        ack_packet @ Packet::Ack(_) => match ack_packet.seq() {
            ack_seq if (ack_seq == current_seq + 1) => Ok(()),
            _else => receive_ack_packet(current_seq, socket)
        },

        Packet::Dat(_, _) => Err(SenderError("Received data packet from listener".into()))?,
        Packet::Fin(_)    => Err(SenderError("Received fin in non fin mode".into()))?,
    }
}


fn init_window(start_seq: Seq) -> Result<(Window, IsEof)> {

    let mut window = Window::with_capacity(WINDOW_SIZE as usize);

    for seq in start_seq..(start_seq + WINDOW_SIZE) {
        let (data_block_size, data_block, is_eof) = read_data_block()?;
        eprintln!("SENDER: read data block size: {}, is_eof: {}", data_block_size, is_eof);

        let packet = Packet::new_dat_packet(seq, data_block);
        eprintln!("SENDER: created new data packet from data block");

        window.push_back(packet);
        eprintln!("SENDER: added packet to window with seq: {}", seq);

        if is_eof { 
            return Ok((window, true)); 
        }
    }

    Ok((window, false))
}


fn send_window(window: &Window, remote_addr: SocketAddr, socket: &UdpSocket) -> Result<()> {
    eprintln!("SENDER: sending window of size: {}", window.len());

    for packet in window {
        send_packet(&packet, remote_addr, socket)?;
        eprintln!("SENDER: send data packet of size: {}", packet.len());
    }

    Ok(())
}


fn slide_window(current_seq: Seq, window: &mut Window, remote_addr: SocketAddr, socket: &UdpSocket) -> Result<IsEof> {
    eprintln!("SENDER: sliding window with seq {}...", current_seq);

    window.pop_front().ok_or(SenderError("Pop failed".into()))?;

    let (data_block_size, data_block, is_eof) = read_data_block()?;
    eprintln!("SENDER: read data block size: {}, is_eof: {}", data_block_size, is_eof);

    let packet = Packet::new_dat_packet(current_seq + WINDOW_SIZE, data_block);
    eprintln!("SENDER: created new data packet from data block");

    send_packet(&packet, remote_addr, socket)?;
    eprintln!("SENDER: send data packet of size: {}", packet.len());
    
    window.push_back(packet);
    Ok(is_eof)
}

fn pop_window(window: &mut Window) -> Result<()> {
    window.pop_front().ok_or(SenderError("Pop failed".into()))?;
    Ok(())
}


// Recursive funstion that slides window and handles
// logic after recieving an ack.
fn transfer_window(current_seq: Seq, window: &mut Window, is_eof: IsEof, remote_addr: SocketAddr, socket: &UdpSocket) -> Result<Seq> {
    match receive_ack_packet(current_seq, socket) {
        Ok(_) => match (is_eof, window.len()) {
            
            // we read everything and the last packet was acked
            (true, 1) => Ok(current_seq),
            
            // we read everything, but still have packets that need to be acked
            (true, _) => {
                pop_window(window)?;
                transfer_window(current_seq + 1, window, is_eof, remote_addr, socket)
            },
            _else => {
                let is_eof = slide_window(current_seq, window, remote_addr, socket)?;
                transfer_window(current_seq + 1, window, is_eof, remote_addr, socket)
            }
        },

        // timeout, try to send the current window again
        Err(SenderError(msg)) => {
            eprintln!("SENDER: failed to read packet with msg: {}", msg);
            eprintln!("SENDER: trying to transfer again");
            send_window(window, remote_addr, socket)?;
            transfer_window(current_seq, window, is_eof, remote_addr, socket)
        }
    }
}

// Start data transfer to remote addr
fn transfer(start_seq: Seq, remote_addr: SocketAddr, socket: &UdpSocket) -> Result<Seq> {
    let (mut window, is_eof) = init_window(start_seq)?;
    send_window(&window, remote_addr, socket)?;
    transfer_window(start_seq, &mut window, is_eof, remote_addr, socket)
}



// FINALIZE ========

fn fin_connection(my_fin_seq: Seq, remote_addr: SocketAddr, socket: &UdpSocket) -> Result<()> {
    
    send_fin_packet(my_fin_seq, remote_addr, socket)?;

    loop {
        eprintln!("SENDER: waiting for ack packet");

        let packet = match read_packet(socket) {
            Ok(packet) => packet,
            Err(SenderError(msg)) => {
                eprintln!("SENDER: failed to read packet: {}", msg);
                send_fin_packet(my_fin_seq, remote_addr, socket)?;
                continue;
            }
        };

        match packet {
            Packet::Ack(_) => { },

            p @ Packet::Fin(_) => {
                let their_fin_seq = p.seq();
                let my_ack_seq = their_fin_seq + 1;
                send_ack_packet(my_ack_seq, remote_addr, socket)?;
                return Ok(());
            },

            Packet::Dat(_, _) => Err(SenderError("Received data packet from listener".into()))?,
        }
    }
}


// MAIN ========

fn main() -> io::Result<()> {

    eprintln!("\nSENDER: starting init...");
    let (start_seq, remote_addr, socket) = match init_connection() {
        Ok((seq, remote_addr, socket)) => (seq, remote_addr, socket),
        Err(SenderError(e)) => panic!("SENDER FATAL: init: {}", e)
    };
    eprintln!("SENDER: finished init!\n");

    eprintln!("SENDER: entered send data loop...");
    let seq = match transfer(start_seq, remote_addr, &socket) {
        Ok(seq) => seq,
        Err(SenderError(e)) => panic!("SENDER FATAL: send: {}", e)
    };
    eprintln!("SENDER: finished data loop!\n");

    eprintln!("SENDER: ending connection...");
    fin_connection(seq, remote_addr, &socket).expect("faild to end");
    eprintln!("SENDER: connection closed!");
    
    Ok(())
}
