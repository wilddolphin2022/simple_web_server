// lib.rs
pub mod server;

#[cfg(test)]

mod tests {
    use crate::server::handle_connection;
    use std::io::Cursor;
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
    use std::pin::Pin;
    use std::task::{Context, Poll};

    struct MockStream {
        read_data: Cursor<Vec<u8>>,
        write_data: Vec<u8>,
    }

    impl AsyncRead for MockStream {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            let this = self.get_mut();
            let n = std::cmp::min(buf.remaining(), this.read_data.get_ref().len() - this.read_data.position() as usize);
            let pos = this.read_data.position();
            buf.put_slice(&this.read_data.get_ref()[pos as usize..pos as usize + n]);
            this.read_data.set_position(pos + n as u64);
            Poll::Ready(Ok(()))
        }
    }

    impl AsyncWrite for MockStream {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, std::io::Error>> {
            self.get_mut().write_data.extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    impl Unpin for MockStream {}

    #[tokio::test]
    async fn test_handle_connection_file_upload() {
        let mut stream = MockStream {
            read_data: Cursor::new(b"POST /files/tone-880.wav HTTP/1.1\r\n".to_vec()),
            write_data: Vec::new(),
        };

        let result = handle_connection(&mut stream).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_connection_file_list() {
        let mut stream = MockStream {
            read_data: Cursor::new(b"GET /files HTTP/1.1\r\n".to_vec()),
            write_data: Vec::new(),
        };

        let result = handle_connection(&mut stream).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_connection_file_content() {
        let mut stream = MockStream {
            read_data: Cursor::new(b"GET /content/tone-880.wav HTTP/1.1\r\n".to_vec()),
            write_data: Vec::new(),
        };

        let result = handle_connection(&mut stream).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_connection_file_metadata() {
        let mut stream = MockStream {
            read_data: Cursor::new(b"GET /metadata/tone-880.wav HTTP/1.1\r\n".to_vec()),
            write_data: Vec::new(),
        };

        let result = handle_connection(&mut stream).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_connection_root() {
        let mut stream = MockStream {
            read_data: Cursor::new(b"GET / HTTP/1.1\r\n".to_vec()),
            write_data: Vec::new(),
        };

        let result = handle_connection(&mut stream).await;
        assert!(result.is_ok());
        assert!(String::from_utf8_lossy(&stream.write_data).starts_with("HTTP/1.1 200 OK\r\n"));
    }

    #[tokio::test]
    async fn test_handle_connection_not_found() {
        let mut stream = MockStream {
            read_data: Cursor::new(b"GET /nonexistent HTTP/1.1\r\n".to_vec()),
            write_data: Vec::new(),
        };

        let result = handle_connection(&mut stream).await;
        assert!(result.is_ok());
        println!("{:?}", String::from_utf8_lossy(&stream.write_data));
        assert!(String::from_utf8_lossy(&stream.write_data).starts_with("HTTP/1.1 404 NOT FOUND\r\n"));
    }
}