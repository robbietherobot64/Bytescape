use clap::{Parser, Subcommand};
use iroh::{Endpoint, SecretKey};
use iroh_docs::{DocTicket, store::Store as DocStore};
use iroh_docs::engine::Engine as DocsEngine;
use iroh_docs::engine::DefaultAuthorStorage;
use iroh_blobs::store::fs::FsStore as BlobStore;
use iroh_blobs::api::downloader::Downloader;
use iroh_gossip::net::Gossip;
use std::path::PathBuf;
use futures::StreamExt;
use std::str::FromStr;

use iroh::address_lookup::mdns::MdnsAddressLookupBuilder;

#[derive(Parser)]
#[command(name = "test-peer")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new document and print the ticket
    Create,
    /// Join a document with a ticket
    Join { ticket: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    // Setup storage
    let data_dir = std::env::temp_dir().join("bytescape-test-peer");
    std::fs::create_dir_all(&data_dir)?;
    println!("Storage: {:?}", data_dir);

    // Initialize Iroh Node with mDNS LAN discovery
    let secret_key = SecretKey::generate(&mut rand::rng());
    let endpoint = Endpoint::builder()
        .secret_key(secret_key)
        .address_lookup(MdnsAddressLookupBuilder::default())
        .bind()
        .await?;
    let node_id = endpoint.secret_key().public();
    println!("NodeID: {}", node_id);

    // Initialize Components (Same stack as Bytescape sync.rs)
    let blob_path = data_dir.join("blobs");
    let blob_store = BlobStore::load(&blob_path).await?;
    let blobs: iroh_blobs::api::Store = blob_store.clone().into();
    
    let doc_path = data_dir.join("docs.db");
    let docs_store = DocStore::persistent(&doc_path)?;
    
    let gossip = Gossip::builder().spawn(endpoint.clone());
    let downloader = Downloader::new(&blobs, &endpoint);
    
    let author_path = data_dir.join("author");
    
    println!("Spawning Docs Engine...");
    let engine = DocsEngine::spawn(
        endpoint.clone(),
        gossip.clone(),
        docs_store,
        blobs.clone(),
        downloader,
        DefaultAuthorStorage::Persistent(author_path),
        None,
    ).await?;

    match cli.command {
        Commands::Create => {
            let author = engine.default_author.get();
            let namespace = engine.sync.import_namespace(iroh_docs::Capability::Write(iroh_docs::NamespaceSecret::new(&mut rand::rng()))).await?;
            engine.sync.open(namespace, Default::default()).await?;
            
            let addr = endpoint.addr();
            let ticket = DocTicket::new(iroh_docs::Capability::Write(engine.sync.export_secret_key(namespace).await?), vec![addr]);
            
            println!("\n✅ Created new document!");
            println!("Ticket: {}", ticket);
            println!("\nRun the Bytescape 'Join Document' command with this ticket to sync.");
            
            // Watch loop
            println!("\nWatching for updates...");
            let (s, r) = async_channel::unbounded();
            engine.sync.subscribe(namespace, s).await?;
            while let Ok(event) = r.recv().await {
                 println!("Event: {:?}", event);
            }
        },
        Commands::Join { ticket } => {
            let ticket = DocTicket::from_str(&ticket)?;
            println!("Joining document...");
            let namespace = engine.sync.import_namespace(ticket.capability.clone()).await?;
            engine.sync.open(namespace, Default::default()).await?;
            
            // Connect to peers
            // Endpoint::add_node_addr does not exist, relying on gossip or manual connect if needed
            // for node in ticket.nodes {
            //    endpoint.add_node_addr(node.into())?;
            // }
            engine.sync.set_sync(namespace, true).await?;
            
            println!("\n✅ Joined document!");
            println!("Namespace: {}", namespace);
            
            // Watch loop
            println!("\nWatching for updates...");
            let (s, r) = async_channel::unbounded();
            engine.sync.subscribe(namespace, s).await?;
             while let Ok(event) = r.recv().await {
                 println!("Event: {:?}", event);
            }
        }
    }

    Ok(())
}
