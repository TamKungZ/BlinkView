use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

const MAGIC: &[u8; 4] = b"BLV1";
const ACK: &[u8; 4] = b"OKAY";

pub enum InstanceRole {
    Primary,
    Forwarded,
}

pub fn become_primary_or_forward(
    port: u16,
    path: Option<&Path>,
    tx: Sender<PathBuf>,
) -> io::Result<InstanceRole> {
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    match TcpListener::bind(addr) {
        Ok(listener) => {
            thread::Builder::new()
                .name("blinkview-ipc".into())
                .spawn(move || server_loop(listener, tx))?;
            Ok(InstanceRole::Primary)
        }
        Err(err) if err.kind() == io::ErrorKind::AddrInUse => {
            ping_or_send(port, path)?;
            Ok(InstanceRole::Forwarded)
        }
        Err(err) => Err(err),
    }
}

fn server_loop(listener: TcpListener, tx: Sender<PathBuf>) {
    for incoming in listener.incoming() {
        let mut stream = match incoming {
            Ok(stream) => stream,
            Err(_) => continue,
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

        let mut magic = [0u8; 4];
        if stream.read_exact(&mut magic).is_err() || &magic != MAGIC {
            continue;
        }

        let mut len_buf = [0u8; 4];
        if stream.read_exact(&mut len_buf).is_err() {
            continue;
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > 1024 * 1024 {
            continue;
        }

        let mut payload = vec![0u8; len];
        if len > 0 && stream.read_exact(&mut payload).is_err() {
            continue;
        }

        if stream.write_all(ACK).is_err() {
            continue;
        }
        let _ = stream.flush();

        if len > 0 {
            let path = decode_path(payload);
            if tx.send(path).is_err() {
                break;
            }
        }
    }
}

fn ping_or_send(port: u16, path: Option<&Path>) -> io::Result<()> {
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    let mut stream = TcpStream::connect_timeout(&addr.into(), Duration::from_millis(600))?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;

    let payload = path.map(encode_path).unwrap_or_default();
    if payload.len() > u32::MAX as usize {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "path is too long"));
    }

    stream.write_all(MAGIC)?;
    stream.write_all(&(payload.len() as u32).to_be_bytes())?;
    if !payload.is_empty() {
        stream.write_all(&payload)?;
    }
    stream.flush()?;

    let mut ack = [0u8; 4];
    stream.read_exact(&mut ack)?;
    if &ack != ACK {
        return Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            format!("TCP port {port} is already used by a non-BlinkView process"),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn encode_path(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(unix)]
fn decode_path(bytes: Vec<u8>) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    PathBuf::from(OsString::from_vec(bytes))
}

#[cfg(windows)]
fn encode_path(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[cfg(windows)]
fn decode_path(bytes: Vec<u8>) -> PathBuf {
    use std::os::windows::ffi::OsStringExt;
    let wide: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    PathBuf::from(OsString::from_wide(&wide))
}

#[cfg(not(any(unix, windows)))]
fn encode_path(path: &Path) -> Vec<u8> {
    path.to_string_lossy().as_bytes().to_vec()
}

#[cfg(not(any(unix, windows)))]
fn decode_path(bytes: Vec<u8>) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(&bytes).into_owned())
}
