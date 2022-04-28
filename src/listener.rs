
// listener.rs

use std::env;
use std::net::{UdpSocket, SocketAddr};
use std::io::{self, Write};
use std::time::Duration;

pub mod protocol;
use protocol::{Packet};

type Result<T> = std::result::Result<T, ListenerError>;
type Seq = u32;


// Custom error for Sender
// implements AddrParseError, io::Error
#[derive(Debug, Clone)]
struct ListenerError(String);

impl From<std::net::AddrParseError> for ListenerError {
    fn from(_: std::net::AddrParseError) -> ListenerError {
        ListenerError("Failed to parse socket address".into())
    }
}

impl From<std::io::Error> for ListenerError {
    fn from(err: std::io::Error) -> ListenerError {
        ListenerError(err.to_string())
    }
}

impl From<protocol::ProtocolError> for ListenerError {
    fn from(err: protocol::ProtocolError) -> ListenerError {
        ListenerError(err.val())
    }
}


// INITIALIZE ========

fn local_port() -> Result<String> {
    let local_port = env::args().nth(1).ok_or(ListenerError("Wrong number of arguments".into()))?;
    eprintln!("LISTENER: parsed local port: {}", local_port);
    Ok(local_port)
}


fn local_addr() -> Result<SocketAddr> {
    use std::str::FromStr;
    let local_addr = format!("127.0.0.1:{}", local_port()?);
    let local_addr = SocketAddr::from_str(&local_addr)?;
    eprintln!("LISTENER: created local addr: {}", local_addr);
    Ok(local_addr)
}


fn open_socket() -> Result<UdpSocket> {
    let socket = UdpSocket::bind(local_addr()?)?;
    eprintln!("LISTENER: opened socket: {}", socket.local_addr()?);
    Ok(socket)
}


// Parse argument to find port, 
// initialize UDP socket with found address
fn init_connection() -> Result<(Seq, UdpSocket)> {

    let start_seq = 0;
    eprintln!("SENDER: initialized seq: {}", start_seq);

    let socket = open_socket()?;

    Ok((start_seq, socket))
}


// STDOUT ========

fn data_packet_to_stdout(packet: Packet) -> Result<usize> {
    eprintln!("LISTENER: printing data to stdout...");
    Ok(io::stdout().write(&packet.data()?)?)
}


// SEND PACKETS ========

fn send_packet(packet: Packet, address: SocketAddr, socket: &UdpSocket) -> Result<usize> {
    let packet_bytes = &packet.to_bytes()[..];
    let packet_size = packet_bytes.len();

    match socket.send_to(packet_bytes, address)? {
        send_size if send_size < packet_size => Err(ListenerError("Did not send packet fully".into())),
        send_size if send_size > packet_size => Err(ListenerError("Send bigger packet than needed".into())),
        send_size => Ok(send_size)
    }
}

fn send_ack_packet(seq: Seq, address: SocketAddr, socket: &UdpSocket) -> Result<()> {
    let packet = protocol::Packet::new_ack_packet(seq);
    send_packet(packet, address, socket)?;
    eprintln!("LISTENER: send ack packet");
    Ok(())
}

fn send_fin_packet(seq: Seq, address: SocketAddr, socket: &UdpSocket) -> Result<()> {
    let packet = protocol::Packet::new_fin_packet(seq);
    send_packet(packet, address, socket)?;
    eprintln!("LISTENER: send fin packet");
    Ok(())
}


// READ PACKETS ========

// Peek without removing header bytes from the stream and return a packet based on them.
fn peek_packet(socket: &UdpSocket) -> Result<Packet> {

    let mut header_bytes = Packet::empty_header_bytes();
    let header_size = socket.peek(&mut header_bytes)?;

    eprintln!("LISTENER: peeked header of size: {}", header_size);

    if Packet::is_header_size(header_size) {
        Ok(Packet::from_header_bytes(header_bytes)?)
    } else {
        Err(ListenerError("peeked header size != header size".into()))
    }
}

// Created a packet by peeking bytes and then update if it is a data packet
fn read_packet(socket: &UdpSocket) -> Result<(Packet, SocketAddr)> {

    let packet = peek_packet(socket)?;
    let packet_len = packet.len();

    let mut packet_bytes = vec![0; packet_len];
    let (packet_size, remote_addr) = socket.recv_from(&mut packet_bytes)?;

    if packet_size != packet_len {
        Err(ListenerError(format!("received packet size({}) != packet length from header({})", packet_size, packet_len)))?
    }

    let packet = match packet {
        data_packet @ Packet::Dat(_, _) => data_packet.clone_with_data(&mut packet_bytes)?,
        non_data_packet => non_data_packet
    };

    Ok((packet, remote_addr))
}


// LOOP ========

fn listen(current_seq: Seq, socket: &UdpSocket) -> Result<(Seq, SocketAddr)> {
    eprintln!("LISTENER: waiting for packet");

    match read_packet(socket) {
        Ok((packet, remote_addr)) => match packet {
            dat_packet @ Packet::Dat(_, _) => {
                eprintln!("LISTENER: received data packet of size: {} from: {}", dat_packet.len(), remote_addr);

                let received_dat_seq = dat_packet.seq();

                // non duplicate received
                if received_dat_seq == current_seq {
                    eprintln!("LISTENER: non duplicate received with seq: {}, current seq: {}", received_dat_seq, current_seq);
                    data_packet_to_stdout(dat_packet)?;
                    let ack_seq = received_dat_seq + 1;
                    send_ack_packet(ack_seq, remote_addr, &socket)?;
                    listen(ack_seq, socket)
                } 
                
                // duplicate received
                else {
                    eprintln!("LISTENER: duplicate received with seq: {}, current seq: {}", received_dat_seq, current_seq);
                    let ack_seq = received_dat_seq + 1;
                    send_ack_packet(ack_seq, remote_addr, &socket)?;
                    listen(current_seq, socket)
                }
            },
    
            fin_packet @ Packet::Fin(_) => {
                eprintln!("LISTENER: received fin packet from: {}", remote_addr);
                Ok((fin_packet.seq(), remote_addr))
            },
    
            Packet::Ack(_) => {
                Err(ListenerError(format!("Received ACK before fin mode from {}", remote_addr)))
            }
        },

        Err(ListenerError(msg)) => {
            eprintln!("LISTENER: failed to read packet with msg: {}", msg);
            listen(current_seq, socket)
        }
    }
}


// FINALIZE ========

fn fin_connection(their_fin_seq: Seq, remote_addr: SocketAddr, socket: &UdpSocket) -> Result<()> {

    while let Err(_) = socket.set_read_timeout(Some(Duration::new(3, 0))) { };

    let my_ack_seq = their_fin_seq + 1;
    send_ack_packet(my_ack_seq, remote_addr, socket)?;

    let my_fin_seq = my_ack_seq + 1;
    send_fin_packet(my_fin_seq, remote_addr, socket)?;

    loop {
        let (packet, remote_addr) = read_packet(socket)?;

        match packet {
            Packet::Ack(_) => return Ok(()),

            Packet::Fin(_) => {
                send_ack_packet(my_ack_seq, remote_addr, socket)?;
                send_fin_packet(my_fin_seq, remote_addr, socket)?;
            },

            Packet::Dat(_, _) => Err(ListenerError("Received Packet::Dat on fin mode".into()))?
        }
    }
}


// MAIN ========

fn main() -> io::Result<()> {

    eprintln!("\nLISTENER: starting init...");
    let (start_seq, socket) = match init_connection() {
        Ok((start_seq, socket)) => (start_seq, socket),
        Err(ListenerError(msg)) => panic!("LISTENER FATAL: socket: {}", msg)
    };
    eprintln!("LISTENER: finished init!\n");

    eprintln!("LISTENER: entered listen data loop...");
    let (fin_seq, remote_addr) = match listen(start_seq, &socket) {
        Ok((fin_seq, remote_addr)) => (fin_seq, remote_addr),
        Err(ListenerError(msg)) => panic!("LISTENER FATAL: loop: {}", msg)
    };
    eprintln!("LISTENER: finished listen loop!\n");

    eprintln!("LISTENER: ending connection...");
    // TODO: make nice
    match fin_connection(fin_seq, remote_addr, &socket) {
        Ok(()) => { } ,
        Err(ListenerError(msg)) => eprintln!("LISTENER ERROR: fin: {}", msg)
    }
    eprintln!("LISTENER: connection closed!");

    Ok(())
}
