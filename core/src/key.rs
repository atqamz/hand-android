use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use iroh::SecretKey;

const KEY_DIR: &str = "iroh";
const KEY_FILE: &str = "companion.key";
const KEY_LEN: usize = 32;

pub trait KeyCipher: Send + Sync {
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, CipherError>;
    fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, CipherError>;
}

#[derive(Debug)]
pub enum CipherError {
    KeyReplaced,
    Corrupt(String),
    Io(io::Error),
}

impl fmt::Display for CipherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeyReplaced => write!(f, "the key that sealed the stored key is gone"),
            Self::Corrupt(message) => write!(f, "{message}"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CipherError {}

impl From<io::Error> for CipherError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn key_path(data_dir: &Path) -> PathBuf {
    data_dir.join(KEY_DIR).join(KEY_FILE)
}

pub fn load_or_create(data_dir: &Path, cipher: &dyn KeyCipher) -> io::Result<SecretKey> {
    let dir = data_dir.join(KEY_DIR);
    create_dir(&dir)?;
    let path = dir.join(KEY_FILE);
    match stored(&path, cipher)? {
        Stored::Loaded(bytes) => Ok(SecretKey::from_bytes(&bytes)),
        Stored::Missing => create(&dir, &path, cipher),
        Stored::Replaced => replace(&dir, &path, cipher),
    }
}

enum Stored {
    Loaded([u8; KEY_LEN]),
    Missing,
    Replaced,
}

fn stored(path: &Path, cipher: &dyn KeyCipher) -> io::Result<Stored> {
    let sealed = match fs::read(path) {
        Ok(sealed) => sealed,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Stored::Missing),
        Err(error) => return Err(error),
    };
    let plaintext = match cipher.open(&sealed) {
        Ok(plaintext) => plaintext,
        Err(CipherError::KeyReplaced) => return Ok(Stored::Replaced),
        Err(CipherError::Corrupt(message)) => return Err(corrupt(path, &message)),
        Err(CipherError::Io(error)) => return Err(error),
    };
    let bytes: [u8; KEY_LEN] = plaintext
        .as_slice()
        .try_into()
        .map_err(|_| corrupt(path, "it does not hold 32 bytes"))?;
    Ok(Stored::Loaded(bytes))
}

fn create(dir: &Path, path: &Path, cipher: &dyn KeyCipher) -> io::Result<SecretKey> {
    let (key, sealed) = sealed_key(cipher)?;
    let temp = write_temp(dir, &key, &sealed)?;
    let linked = fs::hard_link(&temp, path);
    let _ = fs::remove_file(&temp);
    match linked {
        Ok(()) => Ok(key),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => match stored(path, cipher)? {
            Stored::Loaded(bytes) => Ok(SecretKey::from_bytes(&bytes)),
            Stored::Missing | Stored::Replaced => Err(error),
        },
        Err(error) => Err(error),
    }
}

fn replace(dir: &Path, path: &Path, cipher: &dyn KeyCipher) -> io::Result<SecretKey> {
    let (key, sealed) = sealed_key(cipher)?;
    let temp = write_temp(dir, &key, &sealed)?;
    let renamed = fs::rename(&temp, path);
    let _ = fs::remove_file(&temp);
    renamed.map(|()| key)
}

fn sealed_key(cipher: &dyn KeyCipher) -> io::Result<(SecretKey, Vec<u8>)> {
    let key = SecretKey::generate();
    let sealed = cipher.seal(&key.to_bytes()).map_err(|error| match error {
        CipherError::Io(error) => error,
        other => io::Error::other(other.to_string()),
    })?;
    Ok((key, sealed))
}

fn write_temp(dir: &Path, key: &SecretKey, sealed: &[u8]) -> io::Result<PathBuf> {
    let temp = dir.join(format!("{KEY_FILE}.{}.tmp", key.public().fmt_short()));
    let written = create_private(&temp).and_then(|mut file| {
        file.write_all(sealed)?;
        file.sync_all()
    });
    match written {
        Ok(()) => Ok(temp),
        Err(error) => {
            let _ = fs::remove_file(&temp);
            Err(error)
        }
    }
}

fn corrupt(path: &Path, reason: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{} is not a readable key: {reason}", path.display()),
    )
}

#[cfg(unix)]
fn create_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

#[cfg(not(unix))]
fn create_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)
}

#[cfg(unix)]
fn create_private(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn create_private(path: &Path) -> io::Result<fs::File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use super::*;

    struct FakeCipher(u8);

    impl KeyCipher for FakeCipher {
        fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, CipherError> {
            let mut sealed = vec![self.0];
            sealed.extend(plaintext.iter().map(|byte| byte ^ self.0));
            Ok(sealed)
        }

        fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, CipherError> {
            if sealed.len() != KEY_LEN + 1 {
                return Err(CipherError::Corrupt(
                    "the ciphertext has the wrong length".into(),
                ));
            }
            if sealed[0] != self.0 {
                return Err(CipherError::KeyReplaced);
            }
            Ok(sealed[1..].iter().map(|byte| byte ^ self.0).collect())
        }
    }

    fn temp() -> crate::testutil::TempDir {
        crate::testutil::TempDir::new()
    }

    #[test]
    fn creates_one_sealed_key_and_reuses_it() {
        let dir = temp();
        let cipher = FakeCipher(1);
        let first = load_or_create(dir.path(), &cipher).unwrap();
        let second = load_or_create(dir.path(), &cipher).unwrap();
        assert_eq!(first.to_bytes(), second.to_bytes());
        let sealed = fs::read(key_path(dir.path())).unwrap();
        assert_ne!(sealed, first.to_bytes().to_vec());
        assert_eq!(sealed, cipher.seal(&first.to_bytes()).unwrap());
    }

    #[test]
    fn simultaneous_first_runs_agree_on_one_key() {
        let dir = Arc::new(temp());
        let keys: Vec<[u8; 32]> = (0..8)
            .map(|_| {
                let dir = dir.clone();
                thread::spawn(move || {
                    load_or_create(dir.path(), &FakeCipher(1))
                        .unwrap()
                        .to_bytes()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();
        assert!(keys.iter().all(|key| *key == keys[0]));
    }

    #[test]
    fn refuses_corrupt_ciphertext() {
        let dir = temp();
        fs::create_dir_all(dir.path().join(KEY_DIR)).unwrap();
        fs::write(key_path(dir.path()), b"short").unwrap();
        let error = load_or_create(dir.path(), &FakeCipher(1)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("not a readable key"));
    }

    #[test]
    fn a_replaced_keystore_key_makes_a_new_identity() {
        let dir = temp();
        let first = load_or_create(dir.path(), &FakeCipher(1)).unwrap();
        let second = load_or_create(dir.path(), &FakeCipher(2)).unwrap();
        assert_ne!(first.to_bytes(), second.to_bytes());
        assert_eq!(
            fs::read(key_path(dir.path())).unwrap(),
            FakeCipher(2).seal(&second.to_bytes()).unwrap()
        );
        let third = load_or_create(dir.path(), &FakeCipher(2)).unwrap();
        assert_eq!(second.to_bytes(), third.to_bytes());
    }

    #[test]
    fn a_reinstall_makes_a_new_identity() {
        let first = temp();
        let second = temp();
        let cipher = FakeCipher(1);
        let old = load_or_create(first.path(), &cipher).unwrap();
        let new = load_or_create(second.path(), &cipher).unwrap();
        assert_ne!(old.to_bytes(), new.to_bytes());
    }

    #[cfg(unix)]
    #[test]
    fn writes_the_ciphertext_and_its_folder_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp();
        load_or_create(dir.path(), &FakeCipher(1)).unwrap();
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&key_path(dir.path())), 0o600);
        assert_eq!(mode(&dir.path().join(KEY_DIR)), 0o700);
    }
}
