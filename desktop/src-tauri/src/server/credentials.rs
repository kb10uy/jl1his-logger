use keyring::{Entry, Error};

const SERVICE: &str = if cfg!(debug_assertions) {
    "org.kb10uy.jl1his-logger.dev"
} else {
    "org.kb10uy.jl1his-logger"
};

pub fn load(url: &str) -> Result<Option<String>, Error> {
    match Entry::new(SERVICE, url)?.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(Error::NoEntry) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn store(url: &str, token: &str) -> Result<(), Error> {
    Entry::new(SERVICE, url)?.set_password(token)
}

pub fn forget(url: &str) -> Result<(), Error> {
    match Entry::new(SERVICE, url)?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(e) => Err(e),
    }
}
