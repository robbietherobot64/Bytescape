import { useState, useCallback, useRef, useEffect, type CSSProperties } from 'react';

/**
 * PURPOSE
 * - Provide a compact in-app emoji picker with category browsing, keyword search,
 *   and a recently-used rail.
 *
 * HOW IT WORKS
 * - Uses a local static emoji catalog (`EMOJI_CATEGORIES`) plus a runtime-managed
 *   recent list persisted in `localStorage`.
 * - On open, focuses search input and refreshes recents.
 * - Supports close-on-outside-click and Escape handling.
 * - Emits selected emoji through `onSelect`, while persisting recents.
 *
 * RELATED FILES
 * - src/components/ComposeBox.tsx (consumer that can mount this picker in composer UX)
 * - src/App.css (theme CSS variables consumed by inline styles)
 *
 * CROSS-SYSTEM NOTES
 * - This component is frontend-local and does not call backend commands.
 * - Persistence boundary is browser storage (`localStorage`), keyed by `RECENT_KEY`.
 */

// ═══════════════════════════════════════════════════════════════════════════
// Emoji Data
// ═══════════════════════════════════════════════════════════════════════════
// Compact built-in dataset organized by category.
// Each entry: [emoji, keywords] where keywords aid search.

interface EmojiEntry {
    emoji: string;
    keywords: string;
}

interface EmojiCategory {
    id: string;
    icon: string;
    label: string;
    emojis: EmojiEntry[];
}

const EMOJI_CATEGORIES: EmojiCategory[] = [
    {
        id: 'recent',
        icon: '🕐',
        label: 'Recently Used',
        emojis: [], // Populated from localStorage
    },
    {
        id: 'smileys',
        icon: '😀',
        label: 'Smileys & Emotion',
        emojis: [
            { emoji: '😀', keywords: 'grinning happy smile' },
            { emoji: '😃', keywords: 'smiley happy face' },
            { emoji: '😄', keywords: 'smile happy laugh' },
            { emoji: '😁', keywords: 'grin beaming teeth' },
            { emoji: '😆', keywords: 'laughing lol squint' },
            { emoji: '😂', keywords: 'joy tears crying laughing lol' },
            { emoji: '🤣', keywords: 'rofl rolling floor laughing' },
            { emoji: '😊', keywords: 'blush happy smile' },
            { emoji: '😇', keywords: 'angel halo innocent' },
            { emoji: '🙂', keywords: 'slightly smiling' },
            { emoji: '🙃', keywords: 'upside down face' },
            { emoji: '😉', keywords: 'wink' },
            { emoji: '😌', keywords: 'relieved peaceful' },
            { emoji: '😍', keywords: 'heart eyes love' },
            { emoji: '🥰', keywords: 'hearts love affection' },
            { emoji: '😘', keywords: 'kiss blow kiss' },
            { emoji: '😗', keywords: 'kissing whistle' },
            { emoji: '😙', keywords: 'kissing smiling eyes' },
            { emoji: '😚', keywords: 'kissing closed eyes' },
            { emoji: '😋', keywords: 'yum delicious tongue' },
            { emoji: '😛', keywords: 'tongue out playful' },
            { emoji: '😜', keywords: 'wink tongue crazy' },
            { emoji: '🤪', keywords: 'zany wild goofy crazy' },
            { emoji: '😝', keywords: 'tongue squint silly' },
            { emoji: '🤑', keywords: 'money mouth dollar rich' },
            { emoji: '🤗', keywords: 'hugging hug open hands' },
            { emoji: '🤭', keywords: 'hand over mouth oops giggle' },
            { emoji: '🤫', keywords: 'shushing quiet shh' },
            { emoji: '🤔', keywords: 'thinking hmm wonder' },
            { emoji: '🤐', keywords: 'zipper mouth shut' },
            { emoji: '🤨', keywords: 'raised eyebrow skeptic' },
            { emoji: '😐', keywords: 'neutral expressionless' },
            { emoji: '😑', keywords: 'expressionless blank' },
            { emoji: '😶', keywords: 'no mouth silent mute' },
            { emoji: '😏', keywords: 'smirk smug' },
            { emoji: '😒', keywords: 'unamused annoyed' },
            { emoji: '🙄', keywords: 'eye roll whatever' },
            { emoji: '😬', keywords: 'grimace awkward cringe' },
            { emoji: '🤥', keywords: 'lying pinocchio' },
            { emoji: '😌', keywords: 'relieved calm' },
            { emoji: '😔', keywords: 'pensive sad thoughtful' },
            { emoji: '😪', keywords: 'sleepy tired' },
            { emoji: '🤤', keywords: 'drooling hungry' },
            { emoji: '😴', keywords: 'sleeping zzz' },
            { emoji: '😷', keywords: 'mask sick medical' },
            { emoji: '🤒', keywords: 'thermometer sick fever' },
            { emoji: '🤕', keywords: 'bandage hurt injured' },
            { emoji: '🤢', keywords: 'nauseous green sick' },
            { emoji: '🤮', keywords: 'vomiting sick throw up' },
            { emoji: '🥵', keywords: 'hot sweating overheated' },
            { emoji: '🥶', keywords: 'cold freezing frozen' },
            { emoji: '🥴', keywords: 'woozy dizzy drunk' },
            { emoji: '😵', keywords: 'dizzy cross eyes' },
            { emoji: '🤯', keywords: 'mind blown exploding head' },
            { emoji: '🤠', keywords: 'cowboy hat yeehaw' },
            { emoji: '🥳', keywords: 'party celebration horn' },
            { emoji: '🥸', keywords: 'disguise glasses mustache' },
            { emoji: '😎', keywords: 'cool sunglasses' },
            { emoji: '🤓', keywords: 'nerd glasses smart' },
            { emoji: '🧐', keywords: 'monocle investigating' },
            { emoji: '😕', keywords: 'confused puzzled' },
            { emoji: '😟', keywords: 'worried concerned' },
            { emoji: '🙁', keywords: 'slightly frowning' },
            { emoji: '😮', keywords: 'open mouth surprised' },
            { emoji: '😯', keywords: 'hushed surprised' },
            { emoji: '😲', keywords: 'astonished shocked' },
            { emoji: '😳', keywords: 'flushed embarrassed' },
            { emoji: '🥺', keywords: 'pleading puppy eyes' },
            { emoji: '😦', keywords: 'frowning open mouth' },
            { emoji: '😧', keywords: 'anguished upset' },
            { emoji: '😨', keywords: 'fearful scared' },
            { emoji: '😰', keywords: 'anxious sweat' },
            { emoji: '😥', keywords: 'sad relieved' },
            { emoji: '😢', keywords: 'cry crying tear' },
            { emoji: '😭', keywords: 'sobbing loud crying wailing' },
            { emoji: '😱', keywords: 'scream horror' },
            { emoji: '😖', keywords: 'confounded frustrated' },
            { emoji: '😣', keywords: 'persevering struggling' },
            { emoji: '😞', keywords: 'disappointed sad' },
            { emoji: '😓', keywords: 'downcast cold sweat' },
            { emoji: '😩', keywords: 'weary tired' },
            { emoji: '😫', keywords: 'tired face exhausted' },
            { emoji: '🥱', keywords: 'yawning bored tired' },
            { emoji: '😤', keywords: 'huffing angry triumph steam' },
            { emoji: '😡', keywords: 'angry pouting mad' },
            { emoji: '😠', keywords: 'angry face mad' },
            { emoji: '🤬', keywords: 'cursing swearing censored' },
            { emoji: '😈', keywords: 'devil smiling horns' },
            { emoji: '👿', keywords: 'imp angry devil' },
            { emoji: '💀', keywords: 'skull dead death' },
            { emoji: '☠️', keywords: 'skull crossbones death poison' },
            { emoji: '💩', keywords: 'poop poo' },
            { emoji: '🤡', keywords: 'clown face' },
            { emoji: '👹', keywords: 'ogre monster' },
            { emoji: '👻', keywords: 'ghost boo halloween' },
            { emoji: '👽', keywords: 'alien ufo extraterrestrial' },
            { emoji: '🤖', keywords: 'robot bot' },
            { emoji: '💋', keywords: 'kiss mark lips' },
            { emoji: '❤️', keywords: 'red heart love' },
            { emoji: '🧡', keywords: 'orange heart' },
            { emoji: '💛', keywords: 'yellow heart' },
            { emoji: '💚', keywords: 'green heart' },
            { emoji: '💙', keywords: 'blue heart' },
            { emoji: '💜', keywords: 'purple heart' },
            { emoji: '🖤', keywords: 'black heart' },
            { emoji: '🤍', keywords: 'white heart' },
            { emoji: '💔', keywords: 'broken heart' },
            { emoji: '💕', keywords: 'two hearts love' },
            { emoji: '💞', keywords: 'revolving hearts' },
            { emoji: '💓', keywords: 'heartbeat pulsing' },
            { emoji: '💗', keywords: 'growing heart' },
            { emoji: '💖', keywords: 'sparkling heart' },
            { emoji: '💘', keywords: 'heart arrow cupid' },
            { emoji: '💝', keywords: 'heart ribbon gift' },
            { emoji: '🔥', keywords: 'fire flame hot lit' },
            { emoji: '⭐', keywords: 'star' },
            { emoji: '🌟', keywords: 'glowing star sparkle' },
            { emoji: '✨', keywords: 'sparkles magic' },
            { emoji: '💫', keywords: 'dizzy shooting star' },
            { emoji: '💥', keywords: 'boom collision crash' },
            { emoji: '💦', keywords: 'sweat droplets splash' },
            { emoji: '💨', keywords: 'dash wind fast' },
        ],
    },
    {
        id: 'people',
        icon: '👋',
        label: 'People & Body',
        emojis: [
            { emoji: '👋', keywords: 'wave hi hello bye' },
            { emoji: '🤚', keywords: 'raised back of hand' },
            { emoji: '🖐️', keywords: 'hand spread fingers' },
            { emoji: '✋', keywords: 'raised hand stop high five' },
            { emoji: '🖖', keywords: 'vulcan spock' },
            { emoji: '👌', keywords: 'ok perfect' },
            { emoji: '🤌', keywords: 'pinched fingers italian' },
            { emoji: '🤏', keywords: 'pinching small tiny' },
            { emoji: '✌️', keywords: 'victory peace sign' },
            { emoji: '🤞', keywords: 'crossed fingers luck hope' },
            { emoji: '🤟', keywords: 'love you hand gesture' },
            { emoji: '🤘', keywords: 'rock on horns metal' },
            { emoji: '🤙', keywords: 'call me hand shaka' },
            { emoji: '👈', keywords: 'point left' },
            { emoji: '👉', keywords: 'point right' },
            { emoji: '👆', keywords: 'point up' },
            { emoji: '👇', keywords: 'point down' },
            { emoji: '☝️', keywords: 'index up one' },
            { emoji: '👍', keywords: 'thumbs up yes approve like' },
            { emoji: '👎', keywords: 'thumbs down no dislike' },
            { emoji: '✊', keywords: 'fist raised fist bump' },
            { emoji: '👊', keywords: 'fist punch oncoming' },
            { emoji: '🤛', keywords: 'left fist' },
            { emoji: '🤜', keywords: 'right fist' },
            { emoji: '👏', keywords: 'clap applause' },
            { emoji: '🙌', keywords: 'raising hands celebrate' },
            { emoji: '👐', keywords: 'open hands' },
            { emoji: '🤲', keywords: 'palms together' },
            { emoji: '🤝', keywords: 'handshake deal agreement' },
            { emoji: '🙏', keywords: 'pray please thanks folded hands' },
            { emoji: '✍️', keywords: 'writing hand' },
            { emoji: '💪', keywords: 'muscle strong arm flex' },
            { emoji: '🦾', keywords: 'mechanical arm prosthetic' },
            { emoji: '🧠', keywords: 'brain smart think' },
            { emoji: '👀', keywords: 'eyes look see watching' },
            { emoji: '👁️', keywords: 'eye' },
            { emoji: '👅', keywords: 'tongue taste' },
            { emoji: '👄', keywords: 'mouth lips' },
            { emoji: '🫡', keywords: 'salute respect' },
            { emoji: '🫶', keywords: 'heart hands love' },
        ],
    },
    {
        id: 'nature',
        icon: '🌿',
        label: 'Animals & Nature',
        emojis: [
            { emoji: '🐶', keywords: 'dog puppy pet' },
            { emoji: '🐱', keywords: 'cat kitten pet' },
            { emoji: '🐭', keywords: 'mouse rat' },
            { emoji: '🐹', keywords: 'hamster gerbil' },
            { emoji: '🐰', keywords: 'rabbit bunny' },
            { emoji: '🦊', keywords: 'fox' },
            { emoji: '🐻', keywords: 'bear' },
            { emoji: '🐼', keywords: 'panda' },
            { emoji: '🐨', keywords: 'koala' },
            { emoji: '🐯', keywords: 'tiger' },
            { emoji: '🦁', keywords: 'lion king' },
            { emoji: '🐮', keywords: 'cow' },
            { emoji: '🐷', keywords: 'pig oink' },
            { emoji: '🐸', keywords: 'frog toad' },
            { emoji: '🐵', keywords: 'monkey face' },
            { emoji: '🦄', keywords: 'unicorn magic' },
            { emoji: '🐝', keywords: 'bee honey' },
            { emoji: '🐛', keywords: 'bug caterpillar' },
            { emoji: '🦋', keywords: 'butterfly' },
            { emoji: '🐌', keywords: 'snail slow' },
            { emoji: '🐞', keywords: 'ladybug ladybird' },
            { emoji: '🐙', keywords: 'octopus' },
            { emoji: '🐠', keywords: 'fish tropical' },
            { emoji: '🐳', keywords: 'whale spouting' },
            { emoji: '🐋', keywords: 'whale' },
            { emoji: '🦈', keywords: 'shark' },
            { emoji: '🐊', keywords: 'crocodile alligator' },
            { emoji: '🐘', keywords: 'elephant' },
            { emoji: '🦒', keywords: 'giraffe' },
            { emoji: '🌸', keywords: 'cherry blossom flower spring' },
            { emoji: '🌹', keywords: 'rose flower red love' },
            { emoji: '🌻', keywords: 'sunflower happy' },
            { emoji: '🌺', keywords: 'hibiscus flower tropical' },
            { emoji: '🌷', keywords: 'tulip flower' },
            { emoji: '🌱', keywords: 'seedling sprout growing' },
            { emoji: '🌲', keywords: 'evergreen tree pine' },
            { emoji: '🌳', keywords: 'deciduous tree' },
            { emoji: '🍀', keywords: 'four leaf clover lucky' },
            { emoji: '🍁', keywords: 'maple leaf autumn fall' },
            { emoji: '🍂', keywords: 'fallen leaf autumn' },
            { emoji: '🌿', keywords: 'herb plant green' },
            { emoji: '🌵', keywords: 'cactus desert' },
            { emoji: '🍄', keywords: 'mushroom fungus' },
        ],
    },
    {
        id: 'food',
        icon: '🍕',
        label: 'Food & Drink',
        emojis: [
            { emoji: '🍎', keywords: 'apple red fruit' },
            { emoji: '🍊', keywords: 'orange tangerine citrus' },
            { emoji: '🍋', keywords: 'lemon citrus sour' },
            { emoji: '🍌', keywords: 'banana fruit' },
            { emoji: '🍉', keywords: 'watermelon fruit summer' },
            { emoji: '🍇', keywords: 'grapes wine' },
            { emoji: '🍓', keywords: 'strawberry fruit berry' },
            { emoji: '🫐', keywords: 'blueberry' },
            { emoji: '🍑', keywords: 'peach fruit' },
            { emoji: '🥑', keywords: 'avocado guac' },
            { emoji: '🍕', keywords: 'pizza slice' },
            { emoji: '🍔', keywords: 'hamburger burger fast food' },
            { emoji: '🍟', keywords: 'fries french fries fast food' },
            { emoji: '🌭', keywords: 'hotdog hot dog' },
            { emoji: '🥪', keywords: 'sandwich sub' },
            { emoji: '🌮', keywords: 'taco mexican' },
            { emoji: '🌯', keywords: 'burrito wrap' },
            { emoji: '🍿', keywords: 'popcorn movie snack' },
            { emoji: '🍩', keywords: 'donut doughnut' },
            { emoji: '🍪', keywords: 'cookie biscuit' },
            { emoji: '🎂', keywords: 'birthday cake celebration' },
            { emoji: '🍰', keywords: 'cake slice shortcake' },
            { emoji: '🧁', keywords: 'cupcake muffin' },
            { emoji: '🍫', keywords: 'chocolate bar candy' },
            { emoji: '🍬', keywords: 'candy sweet' },
            { emoji: '🍭', keywords: 'lollipop candy' },
            { emoji: '🍦', keywords: 'ice cream soft serve' },
            { emoji: '☕', keywords: 'coffee tea hot drink' },
            { emoji: '🍵', keywords: 'tea green tea matcha' },
            { emoji: '🥤', keywords: 'cup straw drink soda' },
            { emoji: '🧃', keywords: 'juice box' },
            { emoji: '🍺', keywords: 'beer mug' },
            { emoji: '🍻', keywords: 'cheers beers clinking' },
            { emoji: '🥂', keywords: 'champagne cheers toast' },
            { emoji: '🍷', keywords: 'wine glass red wine' },
            { emoji: '🥃', keywords: 'whiskey tumbler' },
        ],
    },
    {
        id: 'travel',
        icon: '✈️',
        label: 'Travel & Places',
        emojis: [
            { emoji: '🚗', keywords: 'car automobile' },
            { emoji: '🚕', keywords: 'taxi cab' },
            { emoji: '🚌', keywords: 'bus' },
            { emoji: '🚎', keywords: 'trolleybus' },
            { emoji: '🏎️', keywords: 'racing car formula' },
            { emoji: '🚑', keywords: 'ambulance' },
            { emoji: '🚒', keywords: 'fire engine truck' },
            { emoji: '✈️', keywords: 'airplane plane flight travel' },
            { emoji: '🚀', keywords: 'rocket ship launch space' },
            { emoji: '🛸', keywords: 'ufo flying saucer alien' },
            { emoji: '🚁', keywords: 'helicopter' },
            { emoji: '⛵', keywords: 'sailboat boat' },
            { emoji: '🚢', keywords: 'ship cruise boat' },
            { emoji: '🚂', keywords: 'train locomotive' },
            { emoji: '🏠', keywords: 'house home' },
            { emoji: '🏡', keywords: 'house garden' },
            { emoji: '🏢', keywords: 'office building' },
            { emoji: '🏰', keywords: 'castle european' },
            { emoji: '🗼', keywords: 'tokyo tower' },
            { emoji: '🗽', keywords: 'statue liberty new york' },
            { emoji: '⛰️', keywords: 'mountain' },
            { emoji: '🏔️', keywords: 'snow capped mountain' },
            { emoji: '🌋', keywords: 'volcano eruption' },
            { emoji: '🏖️', keywords: 'beach umbrella vacation' },
            { emoji: '🏕️', keywords: 'camping tent' },
            { emoji: '🌅', keywords: 'sunrise ocean' },
            { emoji: '🌄', keywords: 'sunrise over mountains' },
            { emoji: '🌠', keywords: 'shooting star night' },
            { emoji: '🌌', keywords: 'milky way galaxy stars' },
            { emoji: '🌈', keywords: 'rainbow colors pride' },
            { emoji: '🎡', keywords: 'ferris wheel carnival' },
            { emoji: '🎢', keywords: 'roller coaster amusement' },
        ],
    },
    {
        id: 'activities',
        icon: '⚽',
        label: 'Activities',
        emojis: [
            { emoji: '⚽', keywords: 'soccer football ball' },
            { emoji: '🏀', keywords: 'basketball ball' },
            { emoji: '🏈', keywords: 'football american' },
            { emoji: '⚾', keywords: 'baseball ball' },
            { emoji: '🥎', keywords: 'softball ball' },
            { emoji: '🎾', keywords: 'tennis ball racket' },
            { emoji: '🏐', keywords: 'volleyball ball' },
            { emoji: '🏓', keywords: 'ping pong table tennis' },
            { emoji: '🥊', keywords: 'boxing gloves' },
            { emoji: '🎮', keywords: 'video game controller gaming' },
            { emoji: '🕹️', keywords: 'joystick arcade game' },
            { emoji: '🎲', keywords: 'dice game' },
            { emoji: '🎯', keywords: 'bullseye target dart' },
            { emoji: '🎳', keywords: 'bowling pin ball' },
            { emoji: '🎸', keywords: 'guitar music rock' },
            { emoji: '🎹', keywords: 'piano keyboard music' },
            { emoji: '🥁', keywords: 'drum music percussion' },
            { emoji: '🎤', keywords: 'microphone karaoke sing' },
            { emoji: '🎧', keywords: 'headphones music listen' },
            { emoji: '🎬', keywords: 'movie clapper film' },
            { emoji: '🎨', keywords: 'art palette paint' },
            { emoji: '🎭', keywords: 'theater drama performing' },
            { emoji: '🏆', keywords: 'trophy winner champion' },
            { emoji: '🥇', keywords: 'gold medal first place' },
            { emoji: '🥈', keywords: 'silver medal second place' },
            { emoji: '🥉', keywords: 'bronze medal third place' },
            { emoji: '🏅', keywords: 'medal sports award' },
            { emoji: '🎪', keywords: 'circus tent' },
            { emoji: '🎉', keywords: 'party popper celebration tada' },
            { emoji: '🎊', keywords: 'confetti celebration' },
            { emoji: '🎈', keywords: 'balloon party' },
            { emoji: '🎁', keywords: 'gift present wrapped' },
            { emoji: '🎀', keywords: 'ribbon bow decoration' },
        ],
    },
    {
        id: 'objects',
        icon: '💡',
        label: 'Objects',
        emojis: [
            { emoji: '💡', keywords: 'light bulb idea' },
            { emoji: '🔦', keywords: 'flashlight torch' },
            { emoji: '📱', keywords: 'phone mobile cell' },
            { emoji: '💻', keywords: 'laptop computer' },
            { emoji: '🖥️', keywords: 'desktop computer monitor' },
            { emoji: '⌨️', keywords: 'keyboard type' },
            { emoji: '🖱️', keywords: 'mouse computer' },
            { emoji: '📷', keywords: 'camera photo' },
            { emoji: '📸', keywords: 'camera flash photo' },
            { emoji: '📹', keywords: 'video camera record' },
            { emoji: '🎥', keywords: 'movie camera film' },
            { emoji: '📺', keywords: 'television tv monitor' },
            { emoji: '📻', keywords: 'radio' },
            { emoji: '📞', keywords: 'telephone phone call' },
            { emoji: '📧', keywords: 'email envelope' },
            { emoji: '📨', keywords: 'incoming envelope mail' },
            { emoji: '📝', keywords: 'memo note writing' },
            { emoji: '📚', keywords: 'books reading study' },
            { emoji: '📖', keywords: 'open book reading' },
            { emoji: '🔑', keywords: 'key lock' },
            { emoji: '🔒', keywords: 'lock locked secure' },
            { emoji: '🔓', keywords: 'unlock unlocked open' },
            { emoji: '🔨', keywords: 'hammer tool' },
            { emoji: '🪓', keywords: 'axe hatchet' },
            { emoji: '⚙️', keywords: 'gear settings' },
            { emoji: '🔧', keywords: 'wrench tool' },
            { emoji: '💎', keywords: 'gem diamond jewel' },
            { emoji: '💰', keywords: 'money bag dollar' },
            { emoji: '💸', keywords: 'money wings flying' },
            { emoji: '🏷️', keywords: 'label tag' },
            { emoji: '🔔', keywords: 'bell notification' },
            { emoji: '🔕', keywords: 'bell muted no sound' },
        ],
    },
    {
        id: 'symbols',
        icon: '❤️',
        label: 'Symbols',
        emojis: [
            { emoji: '❤️', keywords: 'red heart love' },
            { emoji: '💯', keywords: 'hundred percent perfect' },
            { emoji: '✅', keywords: 'check mark done' },
            { emoji: '❌', keywords: 'cross mark wrong no' },
            { emoji: '⭕', keywords: 'circle o' },
            { emoji: '❗', keywords: 'exclamation important' },
            { emoji: '❓', keywords: 'question mark' },
            { emoji: '‼️', keywords: 'double exclamation' },
            { emoji: '⁉️', keywords: 'exclamation question' },
            { emoji: '💤', keywords: 'zzz sleep' },
            { emoji: '💬', keywords: 'speech bubble chat talk' },
            { emoji: '💭', keywords: 'thought bubble think' },
            { emoji: '🗯️', keywords: 'anger bubble mad' },
            { emoji: '♻️', keywords: 'recycle green environment' },
            { emoji: '⚠️', keywords: 'warning caution' },
            { emoji: '🚫', keywords: 'prohibited forbidden no' },
            { emoji: '❎', keywords: 'cross mark square no' },
            { emoji: '🆗', keywords: 'ok button' },
            { emoji: '🆕', keywords: 'new button' },
            { emoji: '🆒', keywords: 'cool button' },
            { emoji: '🆓', keywords: 'free button' },
            { emoji: '🔝', keywords: 'top arrow up' },
            { emoji: '➡️', keywords: 'arrow right' },
            { emoji: '⬅️', keywords: 'arrow left' },
            { emoji: '⬆️', keywords: 'arrow up' },
            { emoji: '⬇️', keywords: 'arrow down' },
            { emoji: '↩️', keywords: 'arrow left curve return' },
            { emoji: '🔀', keywords: 'shuffle twisted arrows' },
            { emoji: '🔁', keywords: 'repeat loop' },
            { emoji: '🎵', keywords: 'musical note music' },
            { emoji: '🎶', keywords: 'musical notes music' },
            { emoji: '♾️', keywords: 'infinity forever' },
        ],
    },
    {
        id: 'flags',
        icon: '🏳️',
        label: 'Flags',
        emojis: [
            { emoji: '🏁', keywords: 'checkered flag race finish' },
            { emoji: '🚩', keywords: 'red flag triangular' },
            { emoji: '🏳️', keywords: 'white flag surrender' },
            { emoji: '🏴', keywords: 'black flag' },
            { emoji: '🏳️‍🌈', keywords: 'rainbow flag pride lgbtq' },
            { emoji: '🏳️‍⚧️', keywords: 'transgender flag pride' },
            { emoji: '🏴‍☠️', keywords: 'pirate flag jolly roger' },
            { emoji: '🇺🇸', keywords: 'us usa united states america' },
            { emoji: '🇬🇧', keywords: 'uk united kingdom britain' },
            { emoji: '🇨🇦', keywords: 'canada' },
            { emoji: '🇦🇺', keywords: 'australia' },
            { emoji: '🇫🇷', keywords: 'france french' },
            { emoji: '🇩🇪', keywords: 'germany german' },
            { emoji: '🇯🇵', keywords: 'japan japanese' },
            { emoji: '🇰🇷', keywords: 'korea south korean' },
            { emoji: '🇧🇷', keywords: 'brazil brazilian' },
            { emoji: '🇲🇽', keywords: 'mexico mexican' },
            { emoji: '🇮🇳', keywords: 'india indian' },
            { emoji: '🇮🇹', keywords: 'italy italian' },
            { emoji: '🇪🇸', keywords: 'spain spanish' },
        ],
    },
];

const RECENT_KEY = 'bytescape-emoji-recent';
const MAX_RECENT = 24;

/** Read recent emoji list from browser storage. */
function getRecentEmojis(): string[] {
    try {
        const saved = localStorage.getItem(RECENT_KEY);
        if (saved) return JSON.parse(saved);
    } catch { /* ignore */ }
    return [];
}

/** Persist a selected emoji at the front of the recent list. */
function saveRecentEmoji(emoji: string) {
    const recent = getRecentEmojis().filter(e => e !== emoji);
    recent.unshift(emoji);
    if (recent.length > MAX_RECENT) recent.length = MAX_RECENT;
    try { localStorage.setItem(RECENT_KEY, JSON.stringify(recent)); } catch { /* quota */ }
    return recent;
}

// ═══════════════════════════════════════════════════════════════════════════
// Styles
// ═══════════════════════════════════════════════════════════════════════════

const pickerContainerStyle: CSSProperties = {
    position: 'absolute',
    bottom: '100%',
    left: 0,
    width: 320,
    maxHeight: 360,
    background: 'var(--bg-panel)',
    border: 'var(--border-default)',
    borderRadius: 'var(--border-radius-md, 4px)',
    boxShadow: '0 4px 16px rgba(0,0,0,0.3)',
    display: 'flex',
    flexDirection: 'column',
    zIndex: 100,
    overflow: 'hidden',
    marginBottom: 4,
};

const searchInputStyle: CSSProperties = {
    width: '100%',
    padding: '8px 10px',
    fontSize: '0.82rem',
    background: 'var(--bg-input)',
    border: 'none',
    borderBottom: 'var(--border-default)',
    color: 'var(--text-primary)',
    fontFamily: 'inherit',
    outline: 'none',
    boxSizing: 'border-box',
};

const categoryTabsStyle: CSSProperties = {
    display: 'flex',
    gap: 0,
    borderBottom: 'var(--border-default)',
    overflowX: 'auto',
    flexShrink: 0,
};

const categoryTabStyle: CSSProperties = {
    padding: '6px 8px',
    background: 'none',
    border: 'none',
    cursor: 'pointer',
    fontSize: '1rem',
    lineHeight: 1,
    opacity: 0.5,
    transition: 'opacity 0.1s',
    flexShrink: 0,
};

const emojiGridStyle: CSSProperties = {
    display: 'grid',
    gridTemplateColumns: 'repeat(8, 1fr)',
    gap: 2,
    padding: '6px 8px',
    overflowY: 'auto',
    flex: 1,
};

const emojiButtonStyle: CSSProperties = {
    background: 'none',
    border: 'none',
    cursor: 'pointer',
    fontSize: '1.3rem',
    lineHeight: 1,
    padding: 4,
    borderRadius: 4,
    transition: 'background 0.1s',
    textAlign: 'center',
};

// ═══════════════════════════════════════════════════════════════════════════
// Component
// ═══════════════════════════════════════════════════════════════════════════

interface EmojiPickerProps {
    onSelect: (emoji: string) => void;
    onClose: () => void;
    visible: boolean;
}

/**
 * Floating emoji picker panel.
 *
 * Contract:
 * - Controlled by `visible`.
 * - Returns selected emoji through `onSelect`.
 * - Requests dismissal through `onClose`.
 */
export function EmojiPicker({ onSelect, onClose, visible }: EmojiPickerProps) {
    const [search, setSearch] = useState('');
    const [activeCategory, setActiveCategory] = useState('smileys');
    const [recentEmojis, setRecentEmojis] = useState<string[]>(getRecentEmojis);
    const containerRef = useRef<HTMLDivElement>(null);
    const searchRef = useRef<HTMLInputElement>(null);
    const gridRef = useRef<HTMLDivElement>(null);

    // Focus search on open
    useEffect(() => {
        if (visible) {
            setSearch('');
            setRecentEmojis(getRecentEmojis());
            setTimeout(() => searchRef.current?.focus(), 50);
        }
    }, [visible]);

    // Click outside to close
    useEffect(() => {
        if (!visible) return;
        const handleClick = (e: MouseEvent) => {
            if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
                onClose();
            }
        };
        // Delay listener to avoid closing immediately
        const timer = setTimeout(() => document.addEventListener('mousedown', handleClick), 0);
        return () => {
            clearTimeout(timer);
            document.removeEventListener('mousedown', handleClick);
        };
    }, [visible, onClose]);

    // Escape to close
    useEffect(() => {
        if (!visible) return;
        const handleKey = (e: KeyboardEvent) => {
            if (e.key === 'Escape') onClose();
        };
        document.addEventListener('keydown', handleKey);
        return () => document.removeEventListener('keydown', handleKey);
    }, [visible, onClose]);

    const handleEmojiClick = useCallback((emoji: string) => {
        const updated = saveRecentEmoji(emoji);
        setRecentEmojis(updated);
        onSelect(emoji);
    }, [onSelect]);

    const handleCategoryClick = useCallback((categoryId: string) => {
        setActiveCategory(categoryId);
        setSearch('');
        // Scroll to top of grid
        if (gridRef.current) gridRef.current.scrollTop = 0;
    }, []);

    if (!visible) return null;

    // Build the display list
    const searchLower = search.toLowerCase().trim();
    let displayEmojis: { emoji: string; category?: string }[] = [];

    if (searchLower) {
        // Search across all categories
        for (const cat of EMOJI_CATEGORIES) {
            if (cat.id === 'recent') continue;
            for (const entry of cat.emojis) {
                if (entry.emoji.includes(searchLower) || entry.keywords.includes(searchLower)) {
                    displayEmojis.push({ emoji: entry.emoji });
                }
            }
        }
    } else if (activeCategory === 'recent') {
        displayEmojis = recentEmojis.map(e => ({ emoji: e }));
    } else {
        const cat = EMOJI_CATEGORIES.find(c => c.id === activeCategory);
        if (cat) {
            displayEmojis = cat.emojis.map(e => ({ emoji: e.emoji }));
        }
    }

    return (
        <div ref={containerRef} style={pickerContainerStyle} data-testid="emoji-picker">
            {/* Search */}
            <input
                ref={searchRef}
                type="text"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search emojis…"
                style={searchInputStyle}
                data-testid="emoji-search"
            />

            {/* Category tabs */}
            <div style={categoryTabsStyle}>
                {EMOJI_CATEGORIES.map((cat) => {
                    // Skip recent if empty
                    if (cat.id === 'recent' && recentEmojis.length === 0) return null;
                    return (
                        <button
                            key={cat.id}
                            onClick={() => handleCategoryClick(cat.id)}
                            style={{
                                ...categoryTabStyle,
                                opacity: activeCategory === cat.id ? 1 : 0.5,
                                borderBottom: activeCategory === cat.id ? '2px solid var(--accent-teal)' : '2px solid transparent',
                            } as CSSProperties}
                            title={cat.label}
                        >
                            {cat.icon}
                        </button>
                    );
                })}
            </div>

            {/* Emoji grid */}
            <div ref={gridRef} style={emojiGridStyle} data-testid="emoji-grid">
                {searchLower && displayEmojis.length === 0 && (
                    <div style={{
                        gridColumn: '1 / -1',
                        textAlign: 'center',
                        padding: 20,
                        color: 'var(--text-muted)',
                        fontSize: '0.78rem',
                    }}>
                        No emojis found for &ldquo;{search}&rdquo;
                    </div>
                )}
                {!searchLower && activeCategory === 'recent' && recentEmojis.length === 0 && (
                    <div style={{
                        gridColumn: '1 / -1',
                        textAlign: 'center',
                        padding: 20,
                        color: 'var(--text-muted)',
                        fontSize: '0.78rem',
                    }}>
                        No recently used emojis yet
                    </div>
                )}
                {displayEmojis.map((item, i) => (
                    <button
                        key={`${item.emoji}-${i}`}
                        onClick={() => handleEmojiClick(item.emoji)}
                        style={emojiButtonStyle}
                        title={item.emoji}
                        type="button"
                    >
                        {item.emoji}
                    </button>
                ))}
            </div>

            {/* Footer with count */}
            {searchLower && displayEmojis.length > 0 && (
                <div style={{
                    padding: '4px 10px',
                    fontSize: '0.65rem',
                    color: 'var(--text-muted)',
                    borderTop: 'var(--border-default)',
                    textAlign: 'right',
                }}>
                    {displayEmojis.length} result{displayEmojis.length !== 1 ? 's' : ''}
                </div>
            )}
        </div>
    );
}
