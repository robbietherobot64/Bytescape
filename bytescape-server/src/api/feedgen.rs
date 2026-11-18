//! Public AT Protocol feed generator.
//!
//! Publishes granular topic feeds (music genres, sports, …) as real
//! `app.bsky.feed.generator` records that anyone can look up and follow from
//! any ATProto client. Endpoints implement the standard feedgen contract:
//! `app.bsky.feed.getFeedGenerator` (self-description) and
//! `app.bsky.feed.getFeed` (feed AT URI → ranked posts).
//!
//! Population source: the `appview_posts` firehose index, matched per topic
//! by hashtag family + keyword rules (TopicDef). Ranked newest-first.

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::AppState;

// ---------------------------------------------------------------------------
// Topic taxonomy — the granular feed catalog. Data, not logic: growing this
// list is how the catalog becomes "every genre, every sport". Rkeys are the
// permanent feed namespace (`app.bsky.feed.generator/<rkey>`) — never rename.
// ---------------------------------------------------------------------------

pub struct TopicDef {
    pub rkey: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// Hashtag family (matched as `#tag` word tokens).
    pub tags: &'static [&'static str],
    /// Plain-text keywords (matched as whole words).
    pub keywords: &'static [&'static str],
}

pub static TOPICS: &[TopicDef] = &[
    // ── Music ──
    TopicDef { rkey: "punk", name: "Punk", description: "Punk rock in all its forms", tags: &["punk", "punkrock", "streetpunk", "crustpunk"], keywords: &["punk"] },
    TopicDef { rkey: "ska", name: "Ska", description: "Ska, ska-punk, two-tone, and the whole rude world", tags: &["ska", "skacore", "skapunk", "twotone", "ska2tone"], keywords: &["ska", "skanking"] },
    TopicDef { rkey: "hardcore", name: "Hardcore", description: "Hardcore punk and heavy breakdowns", tags: &["hardcore", "hardcorepunk", "hxc", "beatdown"], keywords: &["hardcore"] },
    TopicDef { rkey: "metal", name: "Metal", description: "Metal across all subgenres", tags: &["metal", "heavymetal", "metalmusic"], keywords: &["metal"] },
    TopicDef { rkey: "death-metal", name: "Death Metal", description: "Death metal and its brutal descendants", tags: &["deathmetal", "deathmetal, brutaldeath", "osdm"], keywords: &["death metal"] },
    TopicDef { rkey: "black-metal", name: "Black Metal", description: "Black metal, raw and atmospheric", tags: &["blackmetal", "atmosphericblack", "rawblack"], keywords: &["black metal"] },
    TopicDef { rkey: "doom", name: "Doom", description: "Doom, stoner, and sludge — slow and heavy", tags: &["doom", "doommetal", "stonerdoom", "sludge"], keywords: &["doom", "stoner rock"] },
    TopicDef { rkey: "thrash", name: "Thrash", description: "Thrash metal and speed", tags: &["thrash", "thrashmetal", "speedmetal"], keywords: &["thrash"] },
    TopicDef { rkey: "prog-metal", name: "Prog Metal", description: "Progressive metal and technical heaviness", tags: &["progmetal", "progressivemetal", "techdeath"], keywords: &["prog metal"] },
    TopicDef { rkey: "metalcore", name: "Metalcore", description: "Metalcore and modern heavy", tags: &["metalcore", "djent", "modernmetal"], keywords: &["metalcore"] },
    TopicDef { rkey: "post-hardcore", name: "Post-Hardcore", description: "Post-hardcore and its descendants", tags: &["posthardcore", "phc", "swancore"], keywords: &["post-hardcore", "post hardcore"] },
    TopicDef { rkey: "emo", name: "Emo", description: "Emo, midwest emo, and feelings with pedals", tags: &["emo", "emocore", "midwestemo", "emoscreamo"], keywords: &["emo"] },
    TopicDef { rkey: "pop-punk", name: "Pop Punk", description: "Pop punk and its hooks", tags: &["poppunk", "easycore", "poppunkers"], keywords: &["pop punk", "pop-punk"] },
    TopicDef { rkey: "indie", name: "Indie", description: "Indie rock and indie pop", tags: &["indie", "indierock", "indiepop", "diy"], keywords: &["indie"] },
    TopicDef { rkey: "shoegaze", name: "Shoegaze", description: "Shoegaze, dream pop, and walls of reverb", tags: &["shoegaze", "dreampop", "gazegaze"], keywords: &["shoegaze", "dream pop"] },
    TopicDef { rkey: "post-punk", name: "Post-Punk", description: "Post-punk and cold wave", tags: &["postpunk", "coldwave", "deathrock"], keywords: &["post-punk", "post punk"] },
    TopicDef { rkey: "new-wave", name: "New Wave", description: "New wave and synth-driven pop", tags: &["newwave", "synthpop", "80spop"], keywords: &["new wave"] },
    TopicDef { rkey: "synthwave", name: "Synthwave", description: "Synthwave, retrowave, and neon nights", tags: &["synthwave", "retrowave", "outrun"], keywords: &["synthwave", "retrowave"] },
    TopicDef { rkey: "darkwave", name: "Darkwave", description: "Darkwave and shadowy synths", tags: &["darkwave", "darkwaves", "coldwave"], keywords: &["darkwave"] },
    TopicDef { rkey: "goth", name: "Goth", description: "Goth, deathrock, and the darker side", tags: &["goth", "gothmusic", "deathrock", "gothic"], keywords: &["goth"] },
    TopicDef { rkey: "industrial", name: "Industrial", description: "Industrial, EBM, and machine music", tags: &["industrial", "ebm", "industrialmusic", "noise"], keywords: &["industrial"] },
    TopicDef { rkey: "grunge", name: "Grunge", description: "Grunge and 90s alternative heaviness", tags: &["grunge", "90srock", "alternativerock"], keywords: &["grunge"] },
    TopicDef { rkey: "garage", name: "Garage", description: "Garage rock and raw energy", tags: &["garage", "garagerock", "garagerockrevival"], keywords: &["garage rock"] },
    TopicDef { rkey: "psych-rock", name: "Psych Rock", description: "Psychedelic rock and mind-expanding jams", tags: &["psychrock", "psychedelic", "psych", "krautrock"], keywords: &["psychedelic rock", "psych rock"] },
    TopicDef { rkey: "folk-punk", name: "Folk Punk", description: "Folk punk and acoustic rebellion", tags: &["folkpunk", "folkpunk", "anarchopunk"], keywords: &["folk punk", "folk-punk"] },
    TopicDef { rkey: "rockabilly", name: "Rockabilly", description: "Rockabilly, psychobilly, and roots rock", tags: &["rockabilly", "psychobilly", "rootsrock"], keywords: &["rockabilly"] },
    TopicDef { rkey: "rock", name: "Rock", description: "Rock and roll, broadly", tags: &["rock", "rocknroll", "rockandroll"], keywords: &["rock"] },
    TopicDef { rkey: "jazz", name: "Jazz", description: "Jazz in all its eras and styles", tags: &["jazz", "jazzmusic", "bebop", "freejazz"], keywords: &["jazz"] },
    TopicDef { rkey: "blues", name: "Blues", description: "Blues, from Delta to electric", tags: &["blues", "bluesmusic", "deltablues", "bluesrock"], keywords: &["blues"] },
    TopicDef { rkey: "soul", name: "Soul", description: "Soul, Motown, and R&B roots", tags: &["soul", "motown", "soulmusic"], keywords: &["soul"] },
    TopicDef { rkey: "funk", name: "Funk", description: "Funk and the pocket", tags: &["funk", "funkmusic", "groove"], keywords: &["funk"] },
    TopicDef { rkey: "disco", name: "Disco", description: "Disco and its afterlife", tags: &["disco", "discomusic", "nudisco"], keywords: &["disco"] },
    TopicDef { rkey: "house", name: "House", description: "House music and its many rooms", tags: &["house", "housemusic", "deephouse", "techhouse"], keywords: &["house music"] },
    TopicDef { rkey: "techno", name: "Techno", description: "Techno, raw and hypnotic", tags: &["techno", "technomusic", "detroittechno"], keywords: &["techno"] },
    TopicDef { rkey: "dnb", name: "Drum & Bass", description: "Drum & bass, jungle, and breakbeats", tags: &["dnb", "drumandbass", "jungle", "breakbeat"], keywords: &["drum and bass", "drum & bass"] },
    TopicDef { rkey: "dubstep", name: "Dubstep", description: "Dubstep and bass weight", tags: &["dubstep", "140", "bassmusic"], keywords: &["dubstep"] },
    TopicDef { rkey: "hip-hop", name: "Hip-Hop", description: "Hip-hop culture and rap music", tags: &["hiphop", "hiphopculture", "rap", "boombap"], keywords: &["hip-hop", "hip hop"] },
    TopicDef { rkey: "rnb", name: "R&B", description: "Rhythm and blues, modern and classic", tags: &["rnb", "rnbmusic", "randb"], keywords: &["r&b", "rnb"] },
    TopicDef { rkey: "pop", name: "Pop", description: "Pop music and pop discourse", tags: &["pop", "popmusic", "poptimism"], keywords: &["pop music"] },
    TopicDef { rkey: "hyperpop", name: "Hyperpop", description: "Hyperpop and maximalist pop futures", tags: &["hyperpop", "pcmusic", "glitchcore"], keywords: &["hyperpop"] },
    TopicDef { rkey: "k-pop", name: "K-Pop", description: "Korean pop and its fandoms", tags: &["kpop", "kpopfan", "kpoptwitter"], keywords: &["k-pop", "kpop"] },
    TopicDef { rkey: "j-pop", name: "J-Pop", description: "Japanese pop and city pop", tags: &["jpop", "citypop", "jrock"], keywords: &["j-pop", "jpop"] },
    TopicDef { rkey: "reggae", name: "Reggae", description: "Reggae, dub, and dancehall", tags: &["reggae", "dub", "dancehall", "soundsystem"], keywords: &["reggae"] },
    TopicDef { rkey: "afrobeats", name: "Afrobeats", description: "Afrobeats, highlife, and African grooves", tags: &["afrobeats", "highlife", "afropop"], keywords: &["afrobeats"] },
    TopicDef { rkey: "latin", name: "Latin", description: "Latin music — salsa, cumbia, and beyond", tags: &["latin", "salsa", "cumbia", "latinmusic"], keywords: &["salsa", "cumbia"] },
    TopicDef { rkey: "classical", name: "Classical", description: "Classical music and composition", tags: &["classical", "classicalmusic", "composer", "orchestra"], keywords: &["classical"] },
    TopicDef { rkey: "game-music", name: "Game Music", description: "Video game soundtracks and VGM", tags: &["gamemusic", "vgm", "soundtrack", "ost"], keywords: &["game music", "soundtrack"] },
    TopicDef { rkey: "chiptune", name: "Chiptune", description: "Chiptune, 8-bit, and retro consoles", tags: &["chiptune", "8bit", "chipmusic", "demoscene"], keywords: &["chiptune"] },
    TopicDef { rkey: "ambient", name: "Ambient", description: "Ambient, drone, and quiet spaces", tags: &["ambient", "drone", "ambientmusic", "minimalism"], keywords: &["ambient"] },
    TopicDef { rkey: "folk", name: "Folk", description: "Folk, americana, and roots music", tags: &["folk", "folkmusic", "americana", "bluegrass"], keywords: &["folk"] },
    TopicDef { rkey: "country", name: "Country", description: "Country music and its outlaws", tags: &["country", "countrymusic", "outlawcountry"], keywords: &["country music"] },
    TopicDef { rkey: "experimental", name: "Experimental", description: "Experimental and avant-garde music", tags: &["experimental", "experimentalmusic", "avantgarde", "noisemusic"], keywords: &["experimental music"] },
    // ── Sports ──
    TopicDef { rkey: "hockey", name: "Hockey", description: "Ice hockey — NHL and beyond", tags: &["hockey", "icehockey", "nhl", "hockeytwitter"], keywords: &["hockey"] },
    TopicDef { rkey: "field-hockey", name: "Field Hockey", description: "Field hockey", tags: &["fieldhockey", "hockeysport"], keywords: &["field hockey"] },
    TopicDef { rkey: "soccer", name: "Soccer", description: "Association football worldwide", tags: &["soccer", "football", "epl", "laliga", "mls"], keywords: &["soccer"] },
    TopicDef { rkey: "american-football", name: "American Football", description: "NFL and college football", tags: &["nfl", "americanfootball", "collegefootball", "cfb"], keywords: &["football"] },
    TopicDef { rkey: "basketball", name: "Basketball", description: "NBA, WNBA, and hoops everywhere", tags: &["basketball", "nba", "wnba", "euroleague"], keywords: &["basketball"] },
    TopicDef { rkey: "baseball", name: "Baseball", description: "MLB and baseball worldwide", tags: &["baseball", "mlb", "npb", "kbo"], keywords: &["baseball"] },
    TopicDef { rkey: "cricket", name: "Cricket", description: "Cricket — Test, ODI, T20", tags: &["cricket", "testcricket", "t20", "ipl"], keywords: &["cricket"] },
    TopicDef { rkey: "rugby", name: "Rugby", description: "Rugby union and league", tags: &["rugby", "rugbyunion", "rugbyleague", "sixnations"], keywords: &["rugby"] },
    TopicDef { rkey: "tennis", name: "Tennis", description: "Tennis — Grand Slams and the tour", tags: &["tennis", "atp", "wta", "rolandgarros"], keywords: &["tennis"] },
    TopicDef { rkey: "golf", name: "Golf", description: "Golf — PGA and the links", tags: &["golf", "pgatour", "golfing"], keywords: &["golf"] },
    TopicDef { rkey: "disc-golf", name: "Disc Golf", description: "Disc golf and frisbee golf", tags: &["discgolf", "frisbeegolf", "discgolfislife", "dgc"], keywords: &["disc golf", "frisbee golf"] },
    TopicDef { rkey: "ultimate", name: "Ultimate", description: "Ultimate frisbee and the spirit of the game", tags: &["ultimate", "ultimatefrisbee", "ultimatenation"], keywords: &["ultimate frisbee"] },
    TopicDef { rkey: "curling", name: "Curling", description: "Curling — the roaring game", tags: &["curling", "curlingcommunity", "brooms"], keywords: &["curling"] },
    TopicDef { rkey: "bowling", name: "Bowling", description: "Bowling — tenpin and beyond", tags: &["bowling", "bowlingcommunity", "pba"], keywords: &["bowling"] },
    TopicDef { rkey: "skiing", name: "Skiing", description: "Skiing — alpine, nordic, and backcountry", tags: &["skiing", "ski", "alpineskiing", "backcountry"], keywords: &["skiing"] },
    TopicDef { rkey: "snowboarding", name: "Snowboarding", description: "Snowboarding and pow days", tags: &["snowboarding", "snowboard", "powday"], keywords: &["snowboarding"] },
    TopicDef { rkey: "skateboarding", name: "Skateboarding", description: "Skateboarding culture and spots", tags: &["skateboarding", "skate", "skatelife", "skateboard"], keywords: &["skateboarding", "skateboard"] },
    TopicDef { rkey: "figure-skating", name: "Figure Skating", description: "Figure skating and ice dance", tags: &["figureskating", "iceskating", "icedance"], keywords: &["figure skating"] },
    TopicDef { rkey: "cycling", name: "Cycling", description: "Cycling — road, track, and commuter life", tags: &["cycling", "bikes", "roadcycling", "bikelife"], keywords: &["cycling"] },
    TopicDef { rkey: "bmx", name: "BMX", description: "BMX racing and freestyle", tags: &["bmx", "bmxfreestyle", "bmxlife"], keywords: &["bmx"] },
    TopicDef { rkey: "running", name: "Running", description: "Running and the runner community", tags: &["running", "runner", "runnersofbluesky", "trailrunning"], keywords: &["running"] },
    TopicDef { rkey: "triathlon", name: "Triathlon", description: "Triathlon and multisport", tags: &["triathlon", "swimbikerun", "ironman"], keywords: &["triathlon"] },
    TopicDef { rkey: "swimming", name: "Swimming", description: "Swimming and open water", tags: &["swimming", "swimlife", "openwater"], keywords: &["swimming"] },
    TopicDef { rkey: "surfing", name: "Surfing", description: "Surfing and surf culture", tags: &["surfing", "surf", "surflife", "waves"], keywords: &["surfing"] },
    TopicDef { rkey: "climbing", name: "Climbing", description: "Rock climbing and mountaineering", tags: &["climbing", "rockclimbing", "bouldering", "alpinism"], keywords: &["climbing"] },
    TopicDef { rkey: "martial-arts", name: "Martial Arts", description: "Martial arts — striking and grappling", tags: &["martialarts", "bjj", "judo", "karate", "muaythai"], keywords: &["martial arts"] },
    TopicDef { rkey: "boxing", name: "Boxing", description: "Boxing and the sweet science", tags: &["boxing", "boxingtwitter", "pugilism"], keywords: &["boxing"] },
    TopicDef { rkey: "mma", name: "MMA", description: "Mixed martial arts and the cage", tags: &["mma", "ufc", "bellator", "grappling"], keywords: &["mma"] },
    TopicDef { rkey: "wrestling", name: "Wrestling", description: "Pro wrestling and grappling", tags: &["wrestling", "prowrestling", "aew", "wwe"], keywords: &["wrestling"] },
    TopicDef { rkey: "volleyball", name: "Volleyball", description: "Volleyball — indoor and beach", tags: &["volleyball", "beachvolleyball", "ncaavb"], keywords: &["volleyball"] },
    TopicDef { rkey: "motorsport", name: "Motorsport", description: "Motorsport across all series", tags: &["motorsport", "racing", "motorsports"], keywords: &["motorsport", "racing"] },
    TopicDef { rkey: "formula-1", name: "Formula 1", description: "Formula 1 and open-wheel racing", tags: &["f1", "formula1", "f1twitter", "indyCar"], keywords: &["formula 1", "formula one"] },
    TopicDef { rkey: "esports", name: "Esports", description: "Competitive gaming and esports", tags: &["esports", "competitivegaming", "esportstwitter"], keywords: &["esports"] },
    TopicDef { rkey: "speedrunning", name: "Speedrunning", description: "Speedrunning and going fast", tags: &["speedrunning", "speedrun", "gdq"], keywords: &["speedrunning"] },
    TopicDef { rkey: "chess", name: "Chess", description: "Chess — over the board and online", tags: &["chess", "chesstwitter", "chesspunks"], keywords: &["chess"] },
    TopicDef { rkey: "poker", name: "Poker", description: "Poker and the felt", tags: &["poker", "pokerlife", "wsop"], keywords: &["poker"] },
    // ── Interests & culture ──
    TopicDef { rkey: "photography", name: "Photography", description: "Photography — cameras, craft, and the image", tags: &["photography", "photo", "photographer", "believeinfilm"], keywords: &["photography", "photos"] },
    TopicDef { rkey: "cats", name: "Cats", description: "Cats, always", tags: &["cats", "catsofmastodon", "caturday", "catsky"], keywords: &["cats", "kitten"] },
    TopicDef { rkey: "wildlife", name: "Wildlife", description: "Wildlife, nature, and the wild world", tags: &["wildlife", "nature", "naturephotography", "wildlifephotography"], keywords: &["wildlife"] },
    TopicDef { rkey: "science", name: "Science", description: "Science, research, and scicomm", tags: &["science", "research", "scicomm", "sciencetwitter"], keywords: &["science", "research"] },
    TopicDef { rkey: "climate", name: "Climate", description: "Climate, environment, and sustainability", tags: &["climate", "climatechange", "environment", "sustainability"], keywords: &["climate"] },
    TopicDef { rkey: "tech-news", name: "Tech News", description: "Technology news and discourse", tags: &["technews", "tech", "technology", "techtwitter"], keywords: &["technology"] },
    TopicDef { rkey: "open-source", name: "Open Source", description: "Open source, FOSS, and software freedom", tags: &["opensource", "foss", "github", "open"], keywords: &["open source"] },
    TopicDef { rkey: "programming", name: "Programming", description: "Programming and software craft", tags: &["programming", "coding", "dev", "software"], keywords: &["programming", "coding"] },
    TopicDef { rkey: "devops", name: "DevOps", description: "DevOps, Linux, and infrastructure", tags: &["devops", "linux", "sysadmin", "kubernetes"], keywords: &["devops", "linux"] },
    TopicDef { rkey: "ai", name: "AI", description: "AI, machine learning, and LLMs", tags: &["ai", "machinelearning", "llm", "genai"], keywords: &["artificial intelligence"] },
    TopicDef { rkey: "webdev", name: "Web Dev", description: "Web development and frontend", tags: &["webdev", "webdevelopment", "javascript", "frontend"], keywords: &["webdev"] },
    TopicDef { rkey: "gaming", name: "Gaming", description: "Video games and gaming culture", tags: &["gaming", "videogames", "games", "gamedev"], keywords: &["gaming"] },
    TopicDef { rkey: "retro-gaming", name: "Retro Gaming", description: "Retro games and emulation", tags: &["retrogaming", "retro", "emulation", "retrogames"], keywords: &["retro gaming"] },
    // "board" is deliberately absent: it matched "Advisory Board Meeting",
    // "board of directors", etc. — institutional prose, not board games.
    TopicDef { rkey: "tabletop", name: "Tabletop", description: "Board games and tabletop", tags: &["tabletop", "boardgames", "boardgaming"], keywords: &["board games", "board game"] },
    TopicDef { rkey: "rpgs", name: "TTRPGs", description: "D&D, TTRPGs, and tabletop roleplaying", tags: &["ttrpg", "dnd", "rpg", "dungeonsanddragons"], keywords: &["dungeons"] },
    TopicDef { rkey: "mental-health", name: "Mental Health", description: "Mental health, wellness, and care", tags: &["mentalhealth", "selfcare", "therapy", "wellness"], keywords: &["mental health"] },
    TopicDef { rkey: "neurodiversity", name: "Neurodiversity", description: "Neurodiversity, ADHD, and autism", tags: &["neurodiversity", "adhd", "autism", "actuallyautistic"], keywords: &["neurodivergent"] },
    TopicDef { rkey: "gardening", name: "Gardening", description: "Gardens, plants, and growing things", tags: &["gardening", "plants", "garden", "bloomscrolling"], keywords: &["gardening"] },
    TopicDef { rkey: "diy", name: "DIY", description: "DIY, crafts, and making", tags: &["diy", "crafts", "woodworking", "maker"], keywords: &["diy"] },
    TopicDef { rkey: "memes", name: "Memes", description: "Memes, humor, and the internet's best", tags: &["memes", "humor", "funny"], keywords: &["memes"] },
    TopicDef { rkey: "comics", name: "Comics", description: "Comics and webcomics", tags: &["comics", "webcomics", "comicstrips"], keywords: &["comics"] },
    TopicDef { rkey: "books", name: "Books", description: "Books, reading, and literature", tags: &["books", "reading", "literature", "bookstodon"], keywords: &["books"] },
    TopicDef { rkey: "movies", name: "Movies", description: "Movies, film, and cinema", tags: &["movies", "film", "cinema", "letterboxd"], keywords: &["movies"] },
    TopicDef { rkey: "history", name: "History", description: "History and the past", tags: &["history", "histodons", "todayinhistory"], keywords: &["history"] },
    TopicDef { rkey: "fediverse", name: "Fediverse", description: "Fediverse and decentralized social discourse", tags: &["fediverse", "activitypub", "atproto", "decentralized"], keywords: &["fediverse"] },
    // ── Picker interests (every picker interest is a topic) ──
    TopicDef { rkey: "dogs", name: "Dogs", description: "Dogs, good dogs", tags: &["dogs", "dogsofmastodon", "dogsky"], keywords: &["dogs"] },
    TopicDef { rkey: "birds", name: "Birds", description: "Birds, birding, and bird photography", tags: &["birds", "birding", "birdphotography"], keywords: &["birds", "birding"] },
    TopicDef { rkey: "art", name: "Art & Illustration", description: "Art, illustration, and the making of it", tags: &["art", "illustration", "mastoart", "digitalart"], keywords: &["art", "illustration"] },
    TopicDef { rkey: "writing", name: "Writing & Poetry", description: "Writing, poetry, and the writing life", tags: &["writing", "poetry", "writingcommunity", "amwriting"], keywords: &["writing", "poetry"] },
    TopicDef { rkey: "music", name: "General Music", description: "Music, all of it", tags: &["music", "nowplaying", "musicians"], keywords: &["music"] },
    TopicDef { rkey: "electronic", name: "Electronic", description: "Electronic music, synths, and ambient", tags: &["electronicmusic", "synth", "ambient"], keywords: &["electronic", "synth"] },
    TopicDef { rkey: "news", name: "General News", description: "News and headlines", tags: &["news", "headlines", "currentevents"], keywords: &["news"] },
    TopicDef { rkey: "worldnews", name: "World News", description: "World news and geopolitics", tags: &["worldnews", "international", "geopolitics"], keywords: &["world news", "geopolitics"] },
    TopicDef { rkey: "cybersecurity", name: "Cybersecurity", description: "Security, infosec, and hacking", tags: &["cybersecurity", "infosec", "hacking"], keywords: &["cybersecurity", "infosec"] },
    TopicDef { rkey: "sports", name: "General Sports", description: "Sports, all of them", tags: &["sports", "athletics"], keywords: &["sports"] },
    TopicDef { rkey: "football", name: "Football (Soccer)", description: "Football — the beautiful game", tags: &["football", "soccer", "premierleague"], keywords: &["football", "soccer"] },
    TopicDef { rkey: "food", name: "Food & Cooking", description: "Food, cooking, and recipes", tags: &["food", "cooking", "recipes", "foodie"], keywords: &["cooking", "recipes"] },
    TopicDef { rkey: "travel", name: "Travel", description: "Travel and wanderlust", tags: &["travel", "wanderlust", "travelphotography"], keywords: &["travel"] },
    TopicDef { rkey: "fashion", name: "Fashion", description: "Fashion, style, and ootd", tags: &["fashion", "style", "ootd"], keywords: &["fashion"] },
    TopicDef { rkey: "parenting", name: "Parenting", description: "Parenting and family life", tags: &["parenting", "momlife", "dadlife"], keywords: &["parenting"] },
    TopicDef { rkey: "lgbtq", name: "LGBTQ+", description: "LGBTQ+ life and community", tags: &["lgbtq", "lgbt", "queer", "trans"], keywords: &["lgbtq", "queer"] },
    TopicDef { rkey: "disability", name: "Disability & Accessibility", description: "Disability and accessibility advocacy", tags: &["accessibility", "disability"], keywords: &["accessibility", "disability"] },
    TopicDef { rkey: "animalsfunny", name: "Funny Animals", description: "Funny animals and animal memes", tags: &["funnyanimals", "animalmemes"], keywords: &["funny animals"] },

    // ── Animals & Pets (expanded 2026-10-31) ──
    TopicDef { rkey: "rats", name: "Rats", description: "Pet rats and rattit energy", tags: &["rats", "petrats", "rattit", "ratsofinstagram"], keywords: &["rats", "pet rat"] },
    TopicDef { rkey: "hamsters", name: "Hamsters", description: "Hamsters and their tiny worlds", tags: &["hamsters", "hamster", "hamstercare"], keywords: &["hamster", "hamsters"] },
    TopicDef { rkey: "guinea-pigs", name: "Guinea Pigs", description: "Guinea pigs and their wheeks", tags: &["guineapigs", "guineapig", "guineapigsofinstagram"], keywords: &["guinea pig", "guinea pigs"] },
    TopicDef { rkey: "spiders", name: "Spiders", description: "Spiders, tarantulas, and arachnid appreciation", tags: &["spiders", "tarantulas", "arachnids", "spiderbro"], keywords: &["spider", "spiders", "tarantula"] },
    TopicDef { rkey: "reptiles", name: "Reptiles", description: "Reptiles and amphibians — scaled and loved", tags: &["reptiles", "reptilesofinstagram", "herpetology", "amphibians"], keywords: &["reptile", "reptiles", "gecko"] },
    TopicDef { rkey: "snakes", name: "Snakes", description: "Snakes and snake keeping", tags: &["snakes", "snake", "snakekeeping", "snek"], keywords: &["snake", "snakes", "python"] },
    TopicDef { rkey: "fish", name: "Fish", description: "Fish keeping and fishing", tags: &["fish", "fishing", "fishkeeping", "flyfishing"], keywords: &["fish", "fishing"] },
    TopicDef { rkey: "aquariums", name: "Aquariums", description: "Aquariums, aquascaping, and reef tanks", tags: &["aquarium", "aquariums", "aquascaping", "reeftank"], keywords: &["aquarium", "aquascape"] },
    TopicDef { rkey: "marine-life", name: "Marine Life", description: "Ocean life and marine biology", tags: &["marinelife", "oceanlife", "marinebiology", "oceancleanup"], keywords: &["marine life", "ocean"] },
    TopicDef { rkey: "dolphins", name: "Dolphins", description: "Dolphins and dolphin intelligence", tags: &["dolphins", "dolphin"], keywords: &["dolphin", "dolphins"] },
    TopicDef { rkey: "whales", name: "Whales", description: "Whales, whaling politics, and whale song", tags: &["whales", "whale", "orca", "whalesong"], keywords: &["whale", "whales", "orca"] },
    TopicDef { rkey: "horses", name: "Horses", description: "Horses, riding, and equestrian life", tags: &["horses", "horse", "equestrian", "horseriding"], keywords: &["horse", "horses", "equestrian"] },
    TopicDef { rkey: "farm-animals", name: "Farm Animals", description: "Farm life and farm animals", tags: &["farmanimals", "farm life", "farm sactuary", "sanctuary"], keywords: &["farm", "farm animals"] },

    // ── Gaming (expanded 2026-10-31) ──
    TopicDef { rkey: "pc-gaming", name: "PC Gaming", description: "PC gaming, builds, and benchmarks", tags: &["pcgaming", "pcbuild", "master race", "steam"], keywords: &["pc gaming", "pcbuild"] },
    TopicDef { rkey: "xbox", name: "Xbox", description: "Xbox consoles, Game Pass, and community", tags: &["xbox", "xboxgamepass", "seriesx", "gamepass"], keywords: &["xbox", "game pass"] },
    TopicDef { rkey: "playstation", name: "PlayStation", description: "PlayStation consoles and games", tags: &["playstation", "ps5", "ps4", "playstation5"], keywords: &["playstation", "ps5"] },
    TopicDef { rkey: "nintendo-switch", name: "Nintendo Switch", description: "Nintendo Switch and Nintendo games", tags: &["nintendoswitch", "switch", "nintendo", "animalcrossing"], keywords: &["nintendo", "switch"] },
    TopicDef { rkey: "vr-gaming", name: "VR Gaming", description: "Virtual reality gaming and worlds", tags: &["vrgaming", "vr", "virtualreality", "quest"], keywords: &["vr gaming", "virtual reality"] },
    TopicDef { rkey: "mobile-gaming", name: "Mobile Gaming", description: "Games on phones and tablets", tags: &["mobilegaming", "mobilegames", "iosgaming", "androidgames"], keywords: &["mobile gaming", "mobile games"] },
    TopicDef { rkey: "game-dev", name: "Game Development", description: "Making games — design, code, and art", tags: &["gamedev", "gamedevelopment", "indiedev", "gamedesign"], keywords: &["game dev", "game development"] },

    // ── Music (added 2026-10-31) ──
    TopicDef { rkey: "edm", name: "EDM", description: "Electronic dance music and festival culture", tags: &["edm", "dance", "edmfamily", "festival"], keywords: &["edm", "dance music"] },
    TopicDef { rkey: "reggaeton", name: "Reggaeton", description: "Reggaeton and Latin urban beats", tags: &["reggaeton", "latinurban", "urbano"], keywords: &["reggaeton"] },
    TopicDef { rkey: "gospel", name: "Gospel", description: "Gospel music and choir", tags: &["gospel", "gospe music", "choir", "worship"], keywords: &["gospel"] },

    // ── Sports & Fitness (added 2026-10-31) ──
    TopicDef { rkey: "lacrosse", name: "Lacrosse", description: "Lacrosse — field and box", tags: &["lacrosse", "lax", "lacrosselife"], keywords: &["lacrosse", "lax"] },
    TopicDef { rkey: "bodybuilding", name: "Bodybuilding", description: "Bodybuilding and physique training", tags: &["bodybuilding", "physique", "hypertrophy", "gym"], keywords: &["bodybuilding", "physique"] },
    TopicDef { rkey: "crossfit", name: "CrossFit", description: "CrossFit and functional fitness", tags: &["crossfit", "wod", "functionalfitness"], keywords: &["crossfit", "wod"] },
    TopicDef { rkey: "weightlifting", name: "Weightlifting", description: "Powerlifting, Olympic lifting, and strength", tags: &["weightlifting", "powerlifting", "strengthtraining", "lifting"], keywords: &["weightlifting", "powerlifting"] },
    TopicDef { rkey: "fitness", name: "Fitness", description: "General fitness and training", tags: &["fitness", "workout", "training", "fitfam"], keywords: &["fitness", "workout"] },

    // ── Home & Lifestyle (added 2026-10-31) ──
    TopicDef { rkey: "homesteading", name: "Homesteading", description: "Homesteading, self-sufficiency, and land", tags: &["homesteading", "homestead", "selfsufficiency", "growyourown"], keywords: &["homestead", "homesteading"] },
    TopicDef { rkey: "tiny-houses", name: "Tiny Houses", description: "Tiny houses and small-space living", tags: &["tinyhouse", "tinyhouses", "smallspaces", "tinyhomelife"], keywords: &["tiny house", "tiny houses"] },
    TopicDef { rkey: "vanlife", name: "Van Life", description: "Van life and vehicle dwelling", tags: &["vanlife", "vanlifedreams", "campervan", "skoolie"], keywords: &["van life", "vanlife"] },
    TopicDef { rkey: "minimalism", name: "Minimalism", description: "Minimalism and intentional living", tags: &["minimalism", "minimalist", "intentionalliving", "declutter"], keywords: &["minimalism", "minimalist"] },
    TopicDef { rkey: "sustainability", name: "Sustainability", description: "Sustainable living and green tech", tags: &["sustainability", "sustainableliving", "greenliving", "climateaction"], keywords: &["sustainability", "sustainable"] },
    TopicDef { rkey: "zero-waste", name: "Zero Waste", description: "Zero waste and low-waste living", tags: &["zerowaste", "lowwaste", "plasticfree", "refuse"], keywords: &["zero waste", "low waste"] },
    TopicDef { rkey: "thrifting", name: "Thrifting", description: "Thrift stores, secondhand, and vintage finds", tags: &["thrifting", "thrifted", "secondhand", "vintage"], keywords: &["thrift", "thrifting"] },
    TopicDef { rkey: "woodworking", name: "Woodworking", description: "Woodworking, joinery, and shop projects", tags: &["woodworking", "woodwork", "woodworker", "joinery"], keywords: &["woodworking", "woodwork"] },
    TopicDef { rkey: "pottery", name: "Pottery", description: "Pottery, ceramics, and the wheel", tags: &["pottery", "ceramics", "handmade", "potterylife"], keywords: &["pottery", "ceramics"] },
    TopicDef { rkey: "sewing", name: "Sewing", description: "Sewing, garment making, and patterns", tags: &["sewing", "sewist", "handmade", "patternmaking"], keywords: &["sewing", "sewist"] },
    TopicDef { rkey: "knitting", name: "Knitting", description: "Knitting, yarn, and cozy projects", tags: &["knitting", "knittersofinstagram", "yarn", "knit"], keywords: &["knitting", "knit"] },
    TopicDef { rkey: "crochet", name: "Crochet", description: "Crochet, amigurumi, and hooks", tags: &["crochet", "crochetersofinstagram", "amigurumi", "yarn"], keywords: &["crochet", "amigurumi"] },

    // ── Food & Drink (added 2026-10-31) ──
    TopicDef { rkey: "baking", name: "Baking", description: "Baking, bread, and pastry", tags: &["baking", "bread", "sourdough", "pastry"], keywords: &["baking", "sourdough"] },
    TopicDef { rkey: "vegan", name: "Vegan", description: "Vegan food and living", tags: &["vegan", "veganfood", "plantbased", "veganrecipes"], keywords: &["vegan", "plant based"] },
    TopicDef { rkey: "vegetarian", name: "Vegetarian", description: "Vegetarian food and recipes", tags: &["vegetarian", "veggie", "meatless", "vegetarianrecipes"], keywords: &["vegetarian"] },
    TopicDef { rkey: "coffee", name: "Coffee", description: "Coffee, espresso, and café culture", tags: &["coffee", "espresso", "specialtycoffee", "latte"], keywords: &["coffee", "espresso"] },
    TopicDef { rkey: "tea", name: "Tea", description: "Tea — all kinds, brewed and gongfu", tags: &["tea", "teatime", "matcha", "gongfutea"], keywords: &["tea", "matcha"] },
    TopicDef { rkey: "beer", name: "Beer", description: "Beer, brewing, and craft culture", tags: &["beer", "craftbeer", "brewing", "homebrew"], keywords: &["beer", "brewing"] },
    TopicDef { rkey: "wine", name: "Wine", description: "Wine, vineyards, and tasting", tags: &["wine", "winelover", "naturalwine", "sommelier"], keywords: &["wine", "vineyard"] },
    TopicDef { rkey: "cocktails", name: "Cocktails", description: "Cocktails, mixology, and bar craft", tags: &["cocktails", "mixology", "cocktailhour", "bartending"], keywords: &["cocktail", "mixology"] },

    // ── Wellness & Mind (added 2026-10-31) ──
    TopicDef { rkey: "yoga", name: "Yoga", description: "Yoga practice and philosophy", tags: &["yoga", "yogaeveryday", "asana", "yogalife"], keywords: &["yoga", "asana"] },
    TopicDef { rkey: "meditation", name: "Meditation", description: "Meditation and mindfulness practice", tags: &["meditation", "mindfulness", "meditate", "vipassana"], keywords: &["meditation", "mindfulness"] },
    TopicDef { rkey: "nutrition", name: "Nutrition", description: "Nutrition, food science, and fuel", tags: &["nutrition", "foodscience", "macros", "dietitian"], keywords: &["nutrition", "food science"] },
    TopicDef { rkey: "astrology", name: "Astrology", description: "Astrology, charts, and the zodiac", tags: &["astrology", "horoscope", "zodiac", "astrologytwitter"], keywords: &["astrology", "zodiac"] },
    TopicDef { rkey: "spirituality", name: "Spirituality", description: "Spirituality beyond any one tradition", tags: &["spirituality", "spiritual", "spiritualgrowth", "innerwork"], keywords: &["spirituality", "spiritual"] },

    // ── Communities & Identity (expanded 2026-10-31) ──
    TopicDef { rkey: "gay", name: "Gay", description: "Gay community and culture", tags: &["gay", "gaypride", "gaymen", "mlm"], keywords: &["gay"] },
    TopicDef { rkey: "lesbian", name: "Lesbian", description: "Lesbian community and culture", tags: &["lesbian", "lesbians", "wlw", "sapphic"], keywords: &["lesbian", "sapphic"] },
    TopicDef { rkey: "bisexual", name: "Bisexual", description: "Bisexual community and culture", tags: &["bisexual", "bi", "bipride", "bisexuality"], keywords: &["bisexual"] },
    TopicDef { rkey: "transgender", name: "Transgender", description: "Trans lives, rights, and community", tags: &["transgender", "trans", "transrights", "transisbeautiful"], keywords: &["trans", "transgender"] },
    TopicDef { rkey: "asexual", name: "Asexual", description: "Ace and aro-spectrum community", tags: &["asexual", "ace", "acespec", "asexuality"], keywords: &["asexual", "ace"] },
    TopicDef { rkey: "aromantic", name: "Aromantic", description: "Aromantic community and culture", tags: &["aromantic", "aro", "arospec"], keywords: &["aromantic", "aro"] },
    TopicDef { rkey: "pansexual", name: "Pansexual", description: "Pansexual community and culture", tags: &["pansexual", "pan", "panpride"], keywords: &["pansexual"] },
    TopicDef { rkey: "polyamory", name: "Polyamory", description: "Polyamory and ethical non-monogamy", tags: &["polyamory", "polyamorous", "enm", "ethicalnonmonogamy"], keywords: &["polyamory", "polyamorous"] },
    TopicDef { rkey: "nonbinary", name: "Non-Binary", description: "Non-binary and genderqueer community", tags: &["nonbinary", "enby", "genderqueer", "genderfluid"], keywords: &["nonbinary", "non-binary"] },
    TopicDef { rkey: "intersex", name: "Intersex", description: "Intersex community and advocacy", tags: &["intersex", "intersexrights", "endogonadal"], keywords: &["intersex"] },

    // ── Faith & Spirituality (added 2026-10-31) ──
    TopicDef { rkey: "christianity", name: "Christianity", description: "Christian faith, life, and discussion", tags: &["christianity", "christian", "church", "faith"], keywords: &["christian", "christianity"] },
    TopicDef { rkey: "islam", name: "Islam", description: "Muslim faith, life, and community", tags: &["islam", "muslim", "ummah", "ramadan"], keywords: &["islam", "muslim"] },
    TopicDef { rkey: "judaism", name: "Judaism", description: "Jewish faith, life, and culture", tags: &["judaism", "jewish", "torah", "shabbat"], keywords: &["judaism", "jewish"] },
    TopicDef { rkey: "hinduism", name: "Hinduism", description: "Hindu faith, philosophy, and culture", tags: &["hinduism", "hindu", "sanatandharma"], keywords: &["hindu", "hinduism"] },
    TopicDef { rkey: "buddhism", name: "Buddhism", description: "Buddhist practice, philosophy, and community", tags: &["buddhism", "buddhist", "dharma", "sangha"], keywords: &["buddhism", "buddhist"] },
    TopicDef { rkey: "atheism", name: "Atheism", description: "Atheism, secularism, and free thought", tags: &["atheism", "atheist", "secular", "humanism"], keywords: &["atheism", "atheist"] },
    TopicDef { rkey: "paganism", name: "Paganism", description: "Pagan, heathen, and earth-based paths", tags: &["paganism", "pagan", "heathen", "wicca"], keywords: &["pagan", "paganism"] },

    // ── Arts & Culture (expanded 2026-10-31) ──
    TopicDef { rkey: "theater", name: "Theater", description: "Theater, stage, and musicals", tags: &["theater", "theatre", "musicals", "broadway"], keywords: &["theater", "theatre", "musical"] },
    TopicDef { rkey: "dance", name: "Dance", description: "Dance — every style, every stage", tags: &["dance", "dancers", "choreography", "ballet"], keywords: &["dance", "ballet"] },
    TopicDef { rkey: "opera", name: "Opera", description: "Opera and the lyric stage", tags: &["opera", "operahouse", "soprano", "libretto"], keywords: &["opera"] },
    TopicDef { rkey: "architecture", name: "Architecture", description: "Architecture, buildings, and design", tags: &["architecture", "architect", "architecturedesign", "urbanism"], keywords: &["architecture", "architect"] },
    TopicDef { rkey: "anime", name: "Anime", description: "Anime — seasonal, classic, and beyond", tags: &["anime", "anitwitter", "manga", "otaku"], keywords: &["anime"] },
    TopicDef { rkey: "manga", name: "Manga", description: "Manga and comics from Japan", tags: &["manga", "mangarecommendations", "shonen", "seinen"], keywords: &["manga"] },
    TopicDef { rkey: "cartoons", name: "Cartoons", description: "Cartoons, animation, and animated film", tags: &["cartoons", "animation", "animated", "cartoon"], keywords: &["cartoon", "animation"] },
    TopicDef { rkey: "sci-fi", name: "Sci-Fi", description: "Science fiction in books, film, and TV", tags: &["scifi", "scifibooks", "sciencefiction", "speculative"], keywords: &["science fiction", "sci-fi"] },
    TopicDef { rkey: "fantasy", name: "Fantasy", description: "Fantasy worlds, books, and games", tags: &["fantasy", "fantasybooks", "epicfantasy", "worldbuilding"], keywords: &["fantasy"] },
    TopicDef { rkey: "horror", name: "Horror", description: "Horror — film, books, and games", tags: &["horror", "horrorcommunity", "horrorbooks", "spooky"], keywords: &["horror"] },
    TopicDef { rkey: "cosplay", name: "Cosplay", description: "Cosplay craft and convention life", tags: &["cosplay", "cosplayer", "cosplayers", "cosplaylife"], keywords: &["cosplay", "cosplayer"] },

    // ── Technology (expanded 2026-10-31) ──
    TopicDef { rkey: "robotics", name: "Robotics", description: "Robots, automation, and mechatronics", tags: &["robotics", "robots", "automation", "mechatronics"], keywords: &["robotics", "robots"] },
    TopicDef { rkey: "3d-printing", name: "3D Printing", description: "3D printing, printers, and printing fails", tags: &["3dprinting", "3dprinter", "fdm", "resinprinting"], keywords: &["3d printing", "3d printer"] },
    TopicDef { rkey: "data-science", name: "Data Science", description: "Data science, analysis, and visualization", tags: &["datascience", "dataanalysis", "dataviz", "machinelearning"], keywords: &["data science", "data analysis"] },
    TopicDef { rkey: "rust", name: "Rust", description: "The Rust programming language", tags: &["rust", "rustlang", "rustlang", "cargo"], keywords: &["rust", "cargo"] },
    TopicDef { rkey: "javascript", name: "JavaScript", description: "JavaScript, TypeScript, and the web stack", tags: &["javascript", "typescript", "js", "nodejs"], keywords: &["javascript", "typescript"] },
    TopicDef { rkey: "python", name: "Python", description: "Python programming and its ecosystem", tags: &["python", "python3", "pydata", "django"], keywords: &["python"] },
    TopicDef { rkey: "hardware", name: "Hardware", description: "PC hardware, components, and builds", tags: &["hardware", "pcbuild", "gpus", "cpus"], keywords: &["hardware", "gpu"] },

    // ── News & Politics (expanded 2026-10-31) ──
    TopicDef { rkey: "politics", name: "Politics", description: "Politics and policy", tags: &["politics", "policy", "government"], keywords: &["politics", "policy"] },
    TopicDef { rkey: "activism", name: "Activism", description: "Activism, organizing, and movements", tags: &["activism", "organizing", "mutualaid", "protest"], keywords: &["activism", "organizing"] },
    TopicDef { rkey: "unions", name: "Labor & Unions", description: "Labor, unions, and workers' rights", tags: &["unions", "labor", "unionize", "workersrights"], keywords: &["union", "labor"] },
    // "space" as a bare keyword is deliberately absent: it matched interior
    // prose — "adorn your space", venue names like "Evvah space" — not the
    // cosmos. Same call as dropping "board" from tabletop (Advisory Board
    // Meeting). Phrases keep the cosmos recall; #space still matches the tag.
    TopicDef { rkey: "space", name: "Space", description: "Spaceflight, exploration, and the cosmos", tags: &["space", "spaceflight", "spacex", "nasa"], keywords: &["spaceflight", "outer space", "deep space", "space exploration", "space station"] },
    TopicDef { rkey: "astronomy", name: "Astronomy", description: "Astronomy, stargazing, and astrophotography", tags: &["astronomy", "stargazing", "astrophotography", "telescope"], keywords: &["astronomy", "telescope"] },

    // ── Travel & Transport (added 2026-10-31) ──
    TopicDef { rkey: "aviation", name: "Aviation", description: "Aviation, planes, and flying", tags: &["aviation", "planes", "pilot", "avgeek"], keywords: &["aviation", "flying"] },
    TopicDef { rkey: "trains", name: "Trains", description: "Trains, rail, and transit", tags: &["trains", "rail", "railfan", "trainspotting"], keywords: &["train", "trains", "rail"] },
    TopicDef { rkey: "cars", name: "Cars", description: "Cars, car culture, and wrenching", tags: &["cars", "carsofinstagram", "carenthusiast", "jdm"], keywords: &["car", "cars"] },
    TopicDef { rkey: "motorcycles", name: "Motorcycles", description: "Motorcycles and riding", tags: &["motorcycles", "motorcycle", "bikelife", "riders"], keywords: &["motorcycle", "biker"] },
    TopicDef { rkey: "camping", name: "Camping", description: "Camping, tents, and nights outside", tags: &["camping", "camp", "backpacking", "outdoors"], keywords: &["camping", "camp"] },
    TopicDef { rkey: "hiking", name: "Hiking", description: "Hiking, trails, and the outdoors", tags: &["hiking", "hike", "trails", "hikerchat"], keywords: &["hiking", "trail"] },
    TopicDef { rkey: "scuba", name: "Scuba Diving", description: "Scuba diving and the underwater world", tags: &["scuba", "scubadiving", "diving", "padi"], keywords: &["scuba", "diving"] },

    // ── Hobbies & Collecting (added 2026-10-31) ──
    TopicDef { rkey: "puzzles", name: "Puzzles", description: "Jigsaws, crosswords, and puzzle hunts", tags: &["puzzles", "jigsawpuzzles", "crossword", "puzzlehunt"], keywords: &["puzzle", "crossword"] },
    TopicDef { rkey: "vinyl-records", name: "Vinyl Records", description: "Vinyl collecting and turntables", tags: &["vinyl", "vinylrecords", "vinylcollection", "nowspinning"], keywords: &["vinyl", "records"] },
    TopicDef { rkey: "trading-cards", name: "Trading Cards", description: "TCGs and trading card collecting", tags: &["tcg", "tradingcards", "pokemon", "mtg"], keywords: &["trading cards", "tcg"] },
    TopicDef { rkey: "model-building", name: "Model Building", description: "Scale models, miniatures, and dioramas", tags: &["modelbuilding", "scalemodels", "miniatures", "diorama"], keywords: &["model building", "miniatures"] },

    // ── Humor & Pop Culture (expanded 2026-10-31) ──
    TopicDef { rkey: "pop-culture", name: "Pop Culture", description: "Pop culture, fandoms, and the discourse", tags: &["popculture", "fandom", "entertainment"], keywords: &["pop culture", "fandom"] },

    // ── Family (expanded 2026-10-31) ──
    TopicDef { rkey: "pregnancy", name: "Pregnancy", description: "Pregnancy and trying to conceive", tags: &["pregnancy", "pregnant", "ttc", "expecting"], keywords: &["pregnancy", "pregnant"] },
    TopicDef { rkey: "homeschooling", name: "Homeschooling", description: "Homeschooling and home education", tags: &["homeschooling", "homeschool", "homeeducation"], keywords: &["homeschool", "homeschooling"] },

    // ── Style & Self-Expression (expanded 2026-10-31) ──
    TopicDef { rkey: "beauty", name: "Beauty", description: "Beauty, makeup, and skincare culture", tags: &["beauty", "makeup", "beautytips", "mua"], keywords: &["makeup", "beauty"] },
    TopicDef { rkey: "skincare", name: "Skincare", description: "Skincare routines and skin science", tags: &["skincare", "skincareroutine", "skincareaddiction"], keywords: &["skincare", "skin care"] },
    TopicDef { rkey: "tattoos", name: "Tattoos", description: "Tattoos, artists, and tattoo culture", tags: &["tattoos", "tattoo", "tattooart", "ink"], keywords: &["tattoo", "tattoos"] },
];

/// Find a topic by generator record rkey.
pub fn find_topic(rkey: &str) -> Option<&'static TopicDef> {
    TOPICS.iter().find(|t| t.rkey == rkey)
}

/// Interest-picker ids that alias to an existing topic rkey. Topic rkeys are
/// the published generator names — they are never renamed once followed.
const INTEREST_ALIASES: &[(&str, &str)] = &[
    ("retrogaming", "retro-gaming"),
    ("hiphop", "hip-hop"),
    ("technews", "tech-news"),
    ("opensource", "open-source"),
    ("mentalhealth", "mental-health"),
];

/// Resolve an interest-picker id to its topic: every interest IS a topic.
/// Picking an interest subscribes this topic's feed plus its hashtags.
pub fn topic_for_interest(id: &str) -> Option<&'static TopicDef> {
    let rkey = INTEREST_ALIASES
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(_, r)| *r)
        .unwrap_or(id);
    find_topic(rkey)
}

/// at-URI of the published generator for a topic.
pub fn topic_feed_uri(t: &TopicDef) -> String {
    format!("at://{}/app.bsky.feed.generator/{}", SERVICE_DID, t.rkey)
}

/// Case-insensitive text-match regex for one topic. See `combined_regex`.
pub fn topic_regex(t: &TopicDef) -> String {
    combined_regex(&[t])
}

/// Case-insensitive text-match regex for several topics (combo feeds):
/// tags match as `#tag` tokens, keywords as whole words. Boundaries use
/// POSIX classes, not `\b` — Postgres ARE treats `\b` as backspace — and
/// treat `_` as a token character, matching `crate::tags` and the `#tag`
/// extractor (migration 064: `#([A-Za-z0-9_]+)`), so `#tag` never matches
/// `#tag_x`.
pub fn combined_regex(topics: &[&TopicDef]) -> String {
    let mut tags: Vec<String> = Vec::new();
    let mut kws: Vec<String> = Vec::new();
    for t in topics {
        tags.extend(t.tags.iter().map(|x| format!("#{}([^[:alnum:]_]|$)", x)));
        kws.extend(
            t.keywords
                .iter()
                .map(|x| format!("(^|[^[:alnum:]_]){}([^[:alnum:]_]|$)", x)),
        );
    }
    format!("(?i)({}|{})", tags.join("|"), kws.join("|"))
}

/// The ingest trigger's keyword regex (migration 068): one whole-word
/// alternation with **the term in capture group 1** — the trigger reads
/// `m[1]` and writes `'kw:' || <term>`. Non-capturing boundaries keep the
/// term the only group; an earlier shape captured the boundary instead,
/// which filled the tag table with garbage rows and left keyword matching
/// silently dead. Boundaries match `crate::tags` (`_` is a token char).
pub fn kw_regex(terms: &[String]) -> String {
    let mut parts: Vec<String> = terms.iter().map(|x| regex_escape(x)).collect();
    parts.sort_by_key(|s| std::cmp::Reverse(s.len()));
    format!(
        "(?:^|[^[:alnum:]_])({})(?:[^[:alnum:]_]|$)",
        parts.join("|")
    )
}

/// Per-key lookup terms for the extracted tag table: topic tags map to
/// hashtag rows, keywords to 'kw:<term>' rows written by the ingest
/// trigger. This replaces regex matching over post text (12-60s scans
/// whose candidate set was every matching row in history).
pub fn topic_terms(topics: &[&TopicDef]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in topics {
        out.extend(t.tags.iter().map(|x| x.to_lowercase()));
        out.extend(t.keywords.iter().map(|x| format!("kw:{}", x.to_lowercase())));
    }
    out.sort();
    out.dedup();
    out
}

fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if r"\.+*?()|[]{}^$".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Sync the topic vocabulary into the database for the ingest trigger.
/// Single source of truth: TOPICS. Idempotent, runs at startup.
pub async fn sync_topic_terms(pool: &sqlx::PgPool) {
    let mut terms: Vec<String> = Vec::new();
    for t in TOPICS {
        terms.extend(t.keywords.iter().map(|x| x.to_lowercase()));
    }
    terms.sort();
    terms.dedup();
    if let Err(e) = sqlx::query(
        "INSERT INTO appview_topic_terms (term) SELECT unnest($1::text[]) ON CONFLICT DO NOTHING",
    )
    .bind(&terms)
    .execute(pool)
    .await
    {
        tracing::warn!("topic term sync failed: {}", e);
        return;
    }
    // The trigger's keyword regex: one whole-word alternation with the
    // term captured so the trigger knows which terms matched.
    let re = kw_regex(&terms);
    if let Err(e) = sqlx::query(
        "INSERT INTO appview_feed_settings (key, value) VALUES ('kw_regex', $1) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(&re)
    .execute(pool)
    .await
    {
        tracing::warn!("kw_regex sync failed: {}", e);
        return;
    }
    // Keyword rows are derived data; re-derive whenever the regex changes
    // (vocabulary edit). The marker makes the repair crash-safe: a boot that
    // dies mid-repair re-runs it next time. The pre-fix regex captured the
    // boundary instead of the term, so every historical row was garbage
    // ('kw:', 'kw:#', …) and keyword matching silently never worked.
    // Spawned, not awaited — it is the "one heavy scan" migration 068 keeps
    // out of band, and blocking startup here delays the bind.
    let repaired = sqlx::query_scalar::<_, String>(
        "SELECT value FROM appview_feed_settings WHERE key = 'kw_rows_repaired'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    if repaired.as_deref() != Some(re.as_str()) {
        let pool = pool.clone();
        let re = re.clone();
        tokio::spawn(async move { repair_kw_rows(&pool, &re).await });
    }
    tracing::info!("[FEEDGEN] synced {} topic keyword terms", terms.len());
}

/// Purge the garbage `kw:` rows and re-derive keyword rows for the 7-day
/// window readers use (`query_feed_rows`). Day-chunked so each scan is
/// bounded and the firehose ingest trigger gets pool time between chunks —
/// one giant scan here once froze the ingest pipeline. Out of band per
/// migration 068; guarded by a crash-safe marker in `appview_feed_settings`.
async fn repair_kw_rows(pool: &sqlx::PgPool, re: &str) {
    if let Err(e) = sqlx::query("DELETE FROM appview_post_tags WHERE tag LIKE 'kw:%'")
        .execute(pool)
        .await
    {
        tracing::warn!("kw row purge failed: {}", e);
        return;
    }
    for day in 0..7 {
        // Per-chunk scan gate (same rule as query_feed_rows): long scans
        // must not hold pool capacity unguarded — that starved login. The
        // permit is per chunk, not per repair, so feed scans interleave.
        let _permit = SCAN_SLOTS
            .acquire()
            .await
            .expect("semaphore closed");
        let inserted = sqlx::query(
            r#"INSERT INTO appview_post_tags (tag, uri, created_at)
               SELECT DISTINCT 'kw:' || lower(m[1]), p.uri, p.created_at
               FROM appview_posts p,
                    LATERAL regexp_matches(p.text, $1, 'gi') AS m
               WHERE p.text IS NOT NULL
                 AND p.created_at >= now() - ($2 + 1) * interval '1 day'
                 AND p.created_at <  now() -  $2      * interval '1 day'
               ON CONFLICT DO NOTHING"#,
        )
        .bind(re)
        .bind(day)
        .execute(pool)
        .await;
        match inserted {
            Ok(r) => tracing::info!("[FEEDGEN] kw repair day {day}: {} rows", r.rows_affected()),
            Err(e) => {
                tracing::warn!("kw row re-derive failed (day {day}): {e}");
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    if let Err(e) = sqlx::query(
        "INSERT INTO appview_feed_settings (key, value) VALUES ('kw_rows_repaired', $1) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(re)
    .execute(pool)
    .await
    {
        tracing::warn!("kw repair marker failed: {}", e);
    }
}

/// The published identity of this feed service — ALWAYS the production did:web,
/// even in dev: generator records must claim the endpoint the network can
/// actually reach.
const SERVICE_DID: &str = "did:web:feeds.bytescape.social";

fn service_did(_state: &AppState) -> &'static str {
    SERVICE_DID
}

#[derive(Deserialize)]
pub struct FeedQuery {
    pub feed: String,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

#[derive(Deserialize)]
pub struct GenQuery {
    pub feed: String,
}

fn parse_rkey(feed: &str) -> Option<&str> {
    feed.rsplit('/').next().filter(|s| !s.is_empty())
}

fn encode_cursor(offset: usize) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, offset.to_string())
}

fn decode_cursor(cursor: &str) -> usize {
    base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, cursor)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Minimal but valid `app.bsky.feed.defs#postView` from an appview row.
fn post_view(row: &AppviewRow) -> Value {
    let mut record = json!({
        "$type": "app.bsky.feed.post",
        "text": row.text.clone().unwrap_or_default(),
        "createdAt": row.created_at.map(|c| c.to_rfc3339()).unwrap_or_default(),
    });
    if let Some(embeds) = &row.embeds {
        if let Some(embed) = bluesky_embed(embeds) {
            record["embed"] = embed;
        }
    }
    json!({
        "uri": row.uri,
        "cid": row.cid,
        "author": {
            "did": row.author_did,
            "handle": row.author_handle.clone().unwrap_or_else(|| row.author_did.clone()),
        },
        "record": record,
        "likeCount": row.like_count.unwrap_or(0),
        "repostCount": row.repost_count.unwrap_or(0),
        "replyCount": row.reply_count.unwrap_or(0),
        "indexedAt": row.created_at.map(|c| c.to_rfc3339()).unwrap_or_default(),
    })
}

/// Pass through the common image/external embed shapes for client rendering.
fn bluesky_embed(embeds: &Value) -> Option<Value> {
    if let Some(imgs) = embeds.get("images").and_then(|v| v.as_array()) {
        let images: Vec<Value> = imgs
            .iter()
            .map(|i| {
                json!({
                    "alt": i.get("alt").and_then(|v| v.as_str()).unwrap_or(""),
                    "image": i.get("thumb").or_else(|| i.get("fullsize")).cloned().unwrap_or(Value::Null),
                })
            })
            .collect();
        return Some(json!({ "$type": "app.bsky.embed.images", "images": images }));
    }
    if let Some(ext) = embeds.get("external") {
        return Some(json!({
            "$type": "app.bsky.embed.external",
            "external": {
                "uri": ext.get("uri").cloned().unwrap_or(Value::Null),
                "title": ext.get("title").cloned().unwrap_or(Value::Null),
                "description": ext.get("description").cloned().unwrap_or(Value::Null),
            }
        }));
    }
    None
}

#[derive(sqlx::FromRow)]
pub struct AppviewRow {
    uri: String,
    cid: String,
    author_did: String,
    author_handle: Option<String>,
    author_avatar: Option<String>,
    author_display_name: Option<String>,
    text: Option<String>,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    embeds: Option<Value>,
    labels: Option<Value>,
    like_count: Option<i64>,
    repost_count: Option<i64>,
    reply_count: Option<i64>,
}

/// GET /xrpc/app.bsky.feed.describeFeedGenerator — required AT Protocol
/// self-description. The AppView validates feed generators against this; a
/// missing/404 response makes every feed show as "improperly configured".
/// Returns the service DID plus the full list of published feed URIs.
async fn describe_feed_generator(State(state): State<AppState>) -> Json<Value> {
    let feeds: Vec<Value> = TOPICS
        .iter()
        .map(|t| json!({ "uri": topic_feed_uri(t), "name": t.name }))
        .collect();
    Json(json!({
        "did": service_did(&state),
        "feeds": feeds,
    }))
}

/// GET /xrpc/app.bsky.feed.getFeedGenerator — feedgen self-description.
async fn get_feed_generator(
    State(state): State<AppState>,
    Query(q): Query<GenQuery>,
) -> Json<Value> {
    let rkey = parse_rkey(&q.feed).unwrap_or("");
    let known = find_topic(rkey).is_some();
    Json(json!({
        "isOnline": true,
        "isValid": known,
        "did": service_did(&state),
    }))
}

/// GET /xrpc/app.bsky.feed.getFeed — the ranked topic feed.
async fn get_feed(
    State(state): State<AppState>,
    Query(q): Query<FeedQuery>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    let rkey = parse_rkey(&q.feed).unwrap_or("");
    let topics = topics_for_rkey(&state.db, rkey).await.ok_or_else(|| {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(json!({ "error": "UnknownFeed", "message": format!("Unknown feed: {}", rkey) })),
        )
    })?;

    let limit = q.limit.unwrap_or(50).clamp(1, 100) as i64;
    let offset = decode_cursor(q.cursor.as_deref().unwrap_or("")) as i64;
    let rows = query_feed_rows(&state.db, &topic_terms(&topics), limit, offset)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "InternalError", "message": e.to_string() })),
            )
        })?;

    let feed: Vec<Value> = rows.iter().map(|r| json!({ "post": post_view(r) })).collect();
    let cursor = if rows.len() as i64 == limit {
        Some(encode_cursor((offset + limit) as usize))
    } else {
        None
    };

    Ok(Json(json!({
        "feed": feed,
        "cursor": cursor,
    })))
}

// ── Generated feed registry: combos + publication ─────────────────────────

#[derive(sqlx::FromRow, serde::Serialize)]
pub struct ComboRow {
    pub rkey: String,
    pub name: String,
    pub topics: Vec<String>,
}

pub async fn combo_by_rkey(db: &sqlx::PgPool, rkey: &str) -> Option<ComboRow> {
    sqlx::query_as::<_, ComboRow>("SELECT rkey, name, topics FROM feed_combos WHERE rkey = $1")
        .bind(rkey)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

/// True when the rkey names one of our generated feeds (topic or combo).
pub async fn is_our_feed(db: &sqlx::PgPool, rkey: &str) -> bool {
    find_topic(rkey).is_some() || combo_by_rkey(db, rkey).await.is_some()
}

/// Resolve a feed rkey to its topic rule sets (single topic, or a combo's).
pub async fn topics_for_rkey(db: &sqlx::PgPool, rkey: &str) -> Option<Vec<&'static TopicDef>> {
    if let Some(t) = find_topic(rkey) {
        return Some(vec![t]);
    }
    let combo = combo_by_rkey(db, rkey).await?;
    let list: Vec<&'static TopicDef> = combo.topics.iter().filter_map(|k| find_topic(k)).collect();
    if list.is_empty() {
        None
    } else {
        Some(list)
    }
}

/// Shared feed query: newest-first topic-matched rows, NSFW excluded.
/// One or two scans at a time: parallel full-table regex scans on the
/// 4.5M-row firehose index thrash the 1GB box (each concurrent scan made the
/// next slower — 2.8s escalating to 27.9s under two concurrent feeds loads).
pub(crate) static SCAN_SLOTS: std::sync::LazyLock<tokio::sync::Semaphore> =
    std::sync::LazyLock::new(|| tokio::sync::Semaphore::new(2));

pub async fn query_feed_rows(
    db: &sqlx::PgPool,
    terms: &[String],
    limit: i64,
    offset: i64,
) -> Result<Vec<AppviewRow>, sqlx::Error> {
    // Per-key lookup on the extracted tag table (tag, created_at DESC) —
    // the old shape matched a regex against post text and its candidate
    // set was every matching row in history (12-60s). A post matching
    // several terms appears once (DISTINCT ON uri). And `tag = ANY(...)`
    // across many terms materializes millions of tag rows before the LIMIT
    // (measured 21s+ on the 11M-row table), so walk each key's index with
    // its own LIMIT and merge — same pattern as feed::sources.
    let window = (limit.max(0) + offset.max(0)).max(8) as i64;
    let mut candidates: Vec<(String, Option<chrono::DateTime<chrono::Utc>>)> = Vec::new();
    let mut first_err: Option<sqlx::Error> = None;
    for batch in terms.chunks(32) {
        let futs = batch.iter().map(|tag| async move {
            sqlx::query_as::<_, (String, Option<chrono::DateTime<chrono::Utc>>)>(
                r#"SELECT uri, created_at FROM appview_post_tags
                   WHERE tag = $1 AND created_at > now() - interval '7 days'
                   ORDER BY created_at DESC NULLS LAST
                   LIMIT $2"#,
            )
            .bind(tag)
            .bind(window)
            .fetch_all(db)
            .await
        });
        for rows in futures::future::join_all(futs).await {
            match rows {
                Ok(rows) => candidates.extend(rows),
                Err(e) => first_err = Some(e),
            }
        }
    }
    if let Some(e) = first_err {
        return Err(e);
    }
    // One row per uri, keeping the newest match; then the requested page.
    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    candidates.dedup_by(|a, b| a.0 == b.0);
    let page: Vec<String> = candidates
        .into_iter()
        .skip(offset.max(0) as usize)
        .take(limit.max(0) as usize)
        .map(|(uri, _)| uri)
        .collect();
    if page.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        r#"SELECT p.uri, p.cid, p.author_did, p.author_handle, p.author_avatar, p.author_display_name, p.text, p.created_at, p.embeds, p.labels,
                  p.like_count::bigint, p.repost_count::bigint, p.reply_count::bigint
           FROM appview_posts p
           WHERE p.uri = ANY($1::text[])
             AND p.reply_parent IS NULL AND p.text IS NOT NULL AND p.text != ''
             AND {}"#,
        crate::api::feed::filter::sql_labels_clean("p.labels"),
    );
    let mut rows = sqlx::query_as::<_, AppviewRow>(&sql)
        .bind(&page)
        .fetch_all(db)
        .await?;
    // Hydration may drop filtered rows; keep the newest-first page order.
    rows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(rows)
}

/// Feed rows as internal FeedPosts (for the in-process feeds tier).
pub fn rows_to_feed_posts(rows: Vec<AppviewRow>) -> Vec<crate::api::feed::FeedPost> {
    rows.into_iter()
        .filter_map(|r| {
            crate::api::feed::normalize::normalize_appview_post(
                r.uri,
                r.cid,
                r.author_did,
                r.author_handle,
                r.author_avatar,
                r.author_display_name,
                r.text,
                r.created_at,
                r.embeds,
                crate::api::feed::normalize::labels_from_json(&r.labels),
                r.like_count.unwrap_or(0) as i32,
                r.repost_count.unwrap_or(0) as i32,
                r.reply_count.unwrap_or(0) as i32,
            )
        })
        .collect()
}

fn slugify(name: &str) -> String {
    let mut s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    s.trim_matches('-').chars().take(24).collect()
}

/// Publish (idempotently) an `app.bsky.feed.generator` record into the
/// user's own repo via their linked account — "easiest for the user": no new
/// accounts, no client-side record writing. Returns the followable AT URI.
async fn publish_record(
    db: &sqlx::PgPool,
    user_id: uuid::Uuid,
    rkey: &str,
    display_name: &str,
    description: &str,
) -> Result<String, String> {
    use crate::db::accounts::LinkedAccountRow;

    let accounts = LinkedAccountRow::find_by_provider(db, user_id, "bluesky")
        .await
        .map_err(|e| e.to_string())?;
    let acct = accounts.first().ok_or("Link a Bluesky account to publish feeds")?;

    let at_uri = format!("at://{}/app.bsky.feed.generator/{}", acct.external_id, rkey);
    let record = json!({
        "$type": "app.bsky.feed.generator",
        "did": SERVICE_DID,
        "displayName": display_name,
        "description": description,
        "createdAt": chrono::Utc::now().to_rfc3339(),
        "acceptsInteractions": false,
    });
    let body = json!({
        "repo": acct.external_id,
        "collection": "app.bsky.feed.generator",
        "rkey": rkey,
        "record": record,
    });

    let recorded = at_uri.clone();
    // putRecord: re-publishing a topic updates its record in place (and the
    // #update event makes the AppView re-resolve the feed service details).
    let result = match crate::oauth::bluesky::repo_write(db, acct, "com.atproto.repo.putRecord", &body).await {
        Ok(_) => Ok(at_uri),
        Err(e) if e.contains("RecordAlreadyExists") => Ok(at_uri),
        Err(e) => Err(format!("Publish failed: {}", e)),
    };

    // Record the real record URI (publisher repo) — the resolvable form for
    // savedFeedsPrefV2 exports. Re-publishing is idempotent and backfills.
    if result.is_ok() {
        let _ = sqlx::query(
            "INSERT INTO published_feeds (rkey, at_uri) VALUES ($1, $2) \
             ON CONFLICT (rkey) DO UPDATE SET at_uri = EXCLUDED.at_uri, published_at = now()",
        )
        .bind(rkey)
        .bind(&recorded)
        .execute(db)
        .await;
    }

    result
}

/// The real record URI of a published topic feed (from the publisher's repo),
/// captured at publish time.
pub async fn published_uri(db: &sqlx::PgPool, rkey: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT at_uri FROM published_feeds WHERE rkey = $1")
        .bind(rkey)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

#[derive(Deserialize)]
struct PublishReq {
    rkey: String,
}

#[derive(Deserialize)]
struct ComboReq {
    name: String,
    topics: Vec<String>,
}

/// GET /api/feedgen/topics — the generated feed catalog.
async fn list_topics(State(_state): State<AppState>) -> Json<Value> {
    let topics: Vec<Value> = TOPICS
        .iter()
        .map(|t| {
            json!({
                "rkey": t.rkey,
                "name": t.name,
                "description": t.description,
                "tags": t.tags,
            })
        })
        .collect();
    Json(json!({ "topics": topics }))
}

/// POST /api/feedgen/publish — publish one topic's generator record.
async fn publish_topic(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PublishReq>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    let user_id =
        crate::api::feed::extract_user_id(&headers, &state.config.jwt_secret).map_err(|e| {
            (
                axum::http::StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "AuthRequired", "message": e.to_string() })),
            )
        })?;
    let topic = find_topic(&req.rkey).ok_or_else(|| {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(json!({ "error": "UnknownFeed", "message": format!("Unknown topic: {}", req.rkey) })),
        )
    })?;
    let at_uri = publish_record(&state.db, user_id, topic.rkey, topic.name, topic.description)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::BAD_GATEWAY,
                Json(json!({ "error": "PublishFailed", "message": e })),
            )
        })?;
    Ok(Json(json!({ "at_uri": at_uri, "rkey": topic.rkey })))
}

/// POST /api/feedgen/combos — create + publish a combined topic feed.
async fn create_combo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ComboReq>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<Value>)> {
    let user_id =
        crate::api::feed::extract_user_id(&headers, &state.config.jwt_secret).map_err(|e| {
            (
                axum::http::StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "AuthRequired", "message": e.to_string() })),
            )
        })?;
    if !(2..=20).contains(&req.topics.len()) {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": "BadCombo", "message": "Pick between 2 and 20 topics" })),
        ));
    }
    if req.topics.iter().any(|t| find_topic(t).is_none()) {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": "BadCombo", "message": "Unknown topic in selection" })),
        ));
    }
    let name = req.name.trim().chars().take(60).collect::<String>();
    if name.is_empty() {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": "BadCombo", "message": "Name required" })),
        ));
    }

    let suffix = uuid::Uuid::new_v4().simple().to_string()[..6].to_string();
    let rkey = format!("{}-{}", slugify(&name), suffix);
    let topic_names: Vec<&str> = req
        .topics
        .iter()
        .filter_map(|k| find_topic(k).map(|t| t.name))
        .collect();
    let description = format!("A combined Bytescape feed: {}", topic_names.join(", "));

    let at_uri = publish_record(&state.db, user_id, &rkey, &name, &description)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::BAD_GATEWAY,
                Json(json!({ "error": "PublishFailed", "message": e })),
            )
        })?;

    sqlx::query(
        "INSERT INTO feed_combos (user_id, rkey, name, topics, at_uri) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(&rkey)
    .bind(&name)
    .bind(&req.topics)
    .bind(&at_uri)
    .execute(&state.db)
    .await
    .map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "DbError", "message": e.to_string() })),
        )
    })?;

    Ok(Json(json!({ "at_uri": at_uri, "rkey": rkey, "name": name })))
}

/// The public feedgen surface (unauthenticated by design — feeds are for
/// everyone; catalog reads too, so clients can browse before signing in).
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/xrpc/app.bsky.feed.describeFeedGenerator", get(describe_feed_generator))
        .route("/xrpc/app.bsky.feed.getFeedGenerator", get(get_feed_generator))
        .route("/xrpc/app.bsky.feed.getFeed", get(get_feed))
        .route("/api/feedgen/topics", get(list_topics))
        .route("/api/feedgen/publish", post(publish_topic))
        .route("/api/feedgen/combos", post(create_combo))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_lookup_by_rkey() {
        assert!(find_topic("punk").is_some());
        assert!(find_topic("disc-golf").is_some());
        assert!(find_topic("does-not-exist").is_none());
    }

    #[test]
    fn regex_matches_tags_and_keywords() {
        let re = regex::Regex::new(&topic_regex(find_topic("ska").unwrap())).unwrap();
        assert!(re.is_match("#ska tonight"));
        assert!(re.is_match("SKA-PUNK forever"));
        assert!(re.is_match("a ska show"));
        assert!(!re.is_match("the skater landed it"));
    }

    /// The trigger writes 'kw:' || m[1], so group 1 MUST be the matched
    /// term — not the boundary (the bug that filled the tag table with
    /// 'kw:', 'kw:#' garbage and left keyword matching dead).
    #[test]
    fn kw_regex_captures_the_term() {
        let terms: Vec<String> = vec!["outer space".to_string(), "spaceflight".to_string()];
        let re = regex::Regex::new(&kw_regex(&terms)).unwrap();
        let caps = re.captures("we love outer space tonight").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "outer space");
        let caps = re.captures("a spaceflight launch").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "spaceflight");
        // whole-word only
        assert!(!re.is_match("outer spaces"));
    }

    /// "space" as a bare keyword is absent (interior prose like "adorn your
    /// space" is not the cosmos); phrases and the #space tag still match.
    #[test]
    fn space_keyword_is_not_bare_space() {
        let t = find_topic("space").unwrap();
        assert!(!t.keywords.contains(&"space"));
        let re = regex::Regex::new(&topic_regex(t)).unwrap();
        assert!(!re.is_match("adorn your space with art"));
        assert!(re.is_match("the outer space station"));
        assert!(re.is_match("#space"));
    }

    #[test]
    fn cursor_roundtrip() {
        assert_eq!(decode_cursor(&encode_cursor(150)), 150);
    }
}
