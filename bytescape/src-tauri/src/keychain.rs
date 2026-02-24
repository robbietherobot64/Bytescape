use iroh::SecretKey;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct AuthStore {
    pub iroh_secret_key: Option<String>, // Hex encoded
    pub did: Option<String>,
    pub handle: Option<String>,
    pub access_jwt: Option<String>,
    pub refresh_jwt: Option<String>,
}

#[derive(Clone)]
pub struct Keychain {
    file_path: PathBuf,
    cache: Arc<Mutex<AuthStore>>,
}

impl Keychain {
    pub fn new(path: PathBuf) -> Self {
        println!("Keychain: Using file storage at {:?}", path);
        
        let store = if path.exists() {
            match fs::read_to_string(&path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => AuthStore::default(),
            }
        } else {
            AuthStore::default()
        };

        Self {
            file_path: path,
            cache: Arc::new(Mutex::new(store)),
        }
    }

    fn save(&self) -> Result<(), String> {
        let store = self.cache.lock().map_err(|e| e.to_string())?;
        let content = serde_json::to_string_pretty(&*store).map_err(|e| e.to_string())?;
        fs::write(&self.file_path, content).map_err(|e| e.to_string())
    }

    pub fn save_iroh_secret(&self, secret_key: &SecretKey) -> Result<(), String> {
        println!("Keychain: Saving Iroh secret...");
        let secret_hex = hex::encode(secret_key.to_bytes());
        {
            let mut store = self.cache.lock().map_err(|e| e.to_string())?;
            store.iroh_secret_key = Some(secret_hex);
        }
        self.save()
    }

    pub fn load_iroh_secret(&self) -> Result<Option<SecretKey>, String> {
        println!("Keychain: Loading Iroh secret...");
        let store = self.cache.lock().map_err(|e| e.to_string())?;
        match &store.iroh_secret_key {
            Some(hex_str) => {
                println!("Keychain: Iroh secret found.");
                let bytes = hex::decode(hex_str).map_err(|e| e.to_string())?;
                let key = SecretKey::try_from(bytes.as_slice()).map_err(|e| e.to_string())?;
                Ok(Some(key))
            },
            None => {
                println!("Keychain: No Iroh secret found.");
                Ok(None)
            }
        }
    }

    pub fn save_session(&self, did: String, handle: String, access_jwt: String, refresh_jwt: String) -> Result<(), String> {
        println!("Keychain: Saving session...");
        {
            let mut store = self.cache.lock().map_err(|e| e.to_string())?;
            store.did = Some(did);
            store.handle = Some(handle);
            store.access_jwt = Some(access_jwt);
            store.refresh_jwt = Some(refresh_jwt);
        }
        self.save()
    }

    pub fn delete_session(&self) -> Result<(), String> {
        println!("Keychain: Deleting session...");
        {
            let mut store = self.cache.lock().map_err(|e| e.to_string())?;
            store.did = None;
            store.handle = None;
            store.access_jwt = None;
            store.refresh_jwt = None;
        }
        self.save()
    }

    pub fn load_session(&self) -> Result<Option<(String, String)>, String> {
        println!("Keychain: Loading session...");
        let store = self.cache.lock().map_err(|e| e.to_string())?;
        if let (Some(did), Some(handle)) = (&store.did, &store.handle) {
             println!("Keychain: Session found for {}", handle);
             Ok(Some((did.clone(), handle.clone())))
        } else {
             println!("Keychain: No session found.");
             Ok(None)
        }
    }

    pub fn get_access_jwt(&self) -> Result<Option<String>, String> {
        let store = self.cache.lock().map_err(|e| e.to_string())?;
        Ok(store.access_jwt.clone())
    }

    pub fn get_refresh_jwt(&self) -> Result<Option<String>, String> {
        let store = self.cache.lock().map_err(|e| e.to_string())?;
        Ok(store.refresh_jwt.clone())
    }

    pub fn get_did(&self) -> Result<Option<String>, String> {
        let store = self.cache.lock().map_err(|e| e.to_string())?;
        Ok(store.did.clone())
    }
}
