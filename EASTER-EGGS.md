# Easter eggs and secrets

> **Spoiler warning:** this page names places, items and a few story events of *The X-Files Game*.

These are hidden features of the original *The X-Files Game* (1998). We found them while rebuilding its engine for
Shiloh, mostly by reading the game's own data and program rather than by playing. As far as we know, most of them were
never documented before; each section says whether it was **new** to us or **already known** on the web. Unless a
section says otherwise, an egg was found in the game's data and has not been checked by playing the original game,
so a few details may be off. Each section also says whether Shiloh supports the egg.

A note on positions: where a step says "at (x, y)", that is a pixel of the game's 640 × 480 picture, counted from the
top-left corner. In Shiloh, F1 (or Shift+Alt+H) shows the clickable areas, which helps with the tiny ones.

## PDA cheat codes (new)

The PDA takes typed codes. Open the PDA on its Puget Sound map, its Seattle map or its address book, type a code
(upper or lower case does not matter) and press Enter. A wrong code does nothing. The PDA is in your inventory from
the first day, so the codes work at any time.

| code | what it does |
|---|---|
| `warp speed x` | A **level-select** screen: a list of places from the whole game. Clicking one takes you there, with the story set up for that point. |
| `Von` | Unlocks a small hidden button on the Puget Sound map's top edge, at about (439, 15); right-click it for the same level-select screen. |
| `big booty` | An **inventory editor**: a grid of every item in the game; click an icon to give yourself the item or take it away. |
| `superagent` | Gives you all ten FBI gadgets (badge, binoculars, camera, evidence kit, flashlight, gun, handcuffs, lock-pick, night vision, phone). |
| `good idea oLord` | A screen with all of the game's "idea" icons. |
| `paranoia me up` / `paranoia me down` | Sets the hidden Paranoia score high / low. |
| `loss me up` / `loss me down` | Sets the hidden Loss score high / low. |
| `x me up` / `x me down` | Sets the hidden score for X (the supernatural) high / low. |
| `asta me up` / `asta me down` | Sets Asta's hidden relationship score high / low. |
| `action movie` | Turns on "action movie" mode, which only matters at the warehouse door lock (when you are there with Skinner): using anything on the lock then plays the break-in scene. |
| `the nature of infinity is paradox` | Adds a red button to the Seattle map. It seems meant as a shortcut to a phone call with Amis in one chapter; elsewhere it just beeps. |
| `Corey Hart` | Closes the map. A joke code with no other effect. |
| `Mary Astadourian`, `Dana Scully`, `Walter Skinner`, `Armistead Shanks`, `Mark Cook`, `Joan Truit`, `John Amis`, `Arley`, `Smolnikoff`, `Stearns` | A developer test: plays that character's short stock reactions for the current part of the game, one after another. Some of them play nothing or only one clip. |

The four hidden scores steer parts of the story and of the "Subliminal Messages" option (see below). The
level-select screen's picture is named after Jason VandenBerghe, who is in the game's credits.

**Shiloh:** supported. Its tests type `warp speed x`, `big booty`, `superagent` and `paranoia me up` and check the
result.

## Hidden switch to the debug screens (new)

The same developer screens can be opened without typing:

1. Open the PDA's address book.
2. Right-click the tiny 2 × 2-pixel spot at (318, 430). The PDA beeps.
3. Switch to the Puget Sound map. Three invisible buttons now sit on its top edge. Right-click:
   - (417, 15) for the idea-icon screen;
   - (427, 15) for the inventory editor;
   - (439, 15) for the level-select screen.

**Shiloh:** supported; a test walks this whole route.

## Hidden FBI logins (new)

On the FBI computer's login screen, clear the pre-filled name, type one of these names, press Tab, type the password
and press Enter:

| name | password |
|---|---|
| `dwight` | `xyzzy` |
| `nickolai` | `jason` |
| `angelina` | `jason` |
| `twila` | `jason` |

Instead of "wrong password", each one shows a short personal message: a thank-you or a dedication from a developer,
signed "J". `xyzzy` is the classic magic
word from *Colossal Cave Adventure*.

**Shiloh:** supported and tested.

## Known login tricks (already known)

Two login tricks have been on the web for years, and both work: the FBI computer's password `shiloh` for the
pre-filled user, and right-clicking the "o" of "Welcome" on the login screen to skip the login entirely. Shiloh
supports both.

## Scully's laptop and "faith" (partly known)

After you pick up Scully's laptop at the Comity Inn, click its icon (or the laptop on the Field Office desk) to get
its login screen, with Scully's name already filled in. The web says the laptop has no password. That is half
right: nothing opens it, but the game does check one password, `faith`. Typing it still says "wrong password", but
it is remembered: Skinner then has a new line of dialogue about it, even if you never asked him about the password.

The laptop also locks you out: the fourth press of OK (counting every try, the right one included) shows a lockout
message and disables both fields until you quit the game.

**Shiloh:** supported and tested. One difference: in Shiloh, loading a saved game resets the lockout.

## Mel's photos (already known)

At Rauch's house, in the cutscene where Willmore gets a phone call from Scully, hold the gun and click the woman in
brown at about 5 seconds into the scene (the window is about half a second long). Instead of a shooting, one of five
photos of her appears at random. She is Melanie McClaire, the game's lead programmer.

**Shiloh:** supported (not separately tested).

## Willmore's suicide notes (already known; one web claim corrected)

Clicking Willmore himself with the gun during a cutscene ends the game with a suicide note, picked at random from
three texts. The web says the note depends on your answers to Cook; in the data, the choice is random.

**Shiloh:** supported, but the note is drawn in Shiloh's own font instead of the game's handwriting font.

## Developers in the criminal database (already known)

On the FBI computer, search the ING **Criminal** database for a member of the development team (as named in the
credits) and you get a mugshot with joke vital statistics and notes, 42 of them in all. Nicknames work too, for
example `Bunny slayer`, `Mel`, `Bob`, `Boba Fett`, `Frosty`, `pez boy`, `Sound God`, `towing queen`,
`Red Goddess`, `asset wrangler`, and (as TV Tropes reports) `Wanker`. E-mailing one of these records to a contact
gets a joke reply.

**Shiloh:** supported.

## Developer bios in the media search (partly known: the web did not know how to trigger them)

The FBI computer's media search has six hidden developer bios. Search for:

| bio | search words |
|---|---|
| Melanie McClaire | `Melanie McClaire`, `McClaire`, `Mel`, `Bob` |
| Jason VandenBerghe | `Jason VandenBerghe`, `Vandenberghe`, `Kayla` |
| Derek Rhys Creech | `Derek Rhys Creech`, `Creech`, `Frosty` |
| Pete Isensee | `Pete Isensee`, `Isensee`, `pez boy` |
| Joseph Kim | `Joseph Kim`, `Kim`, `asset wrangler` |
| Alex Smits | `Alex Smits`, `Smits`, `Bunny slayer` |

**Shiloh:** supported.

## The personal call (new)

Barbara Willmore's number is on her card in the PDA address book. Dial it on the cell phone
(206-555-0179). After her message, an e-mail from the FBI Accounting Office arrives: the personal call on the
government phone will cost you 25 cents off your next paycheck.

**Shiloh:** supported.

## The Lone Gunmen (already known; number corrected)

From the Comity Inn chapter on, you can call the Lone Gunmen once on the cell phone. The number usually posted on the
web ends in 0147; the game's number is 1-202-555-0149.

**Shiloh:** supported.

## The studio's address in the PDA (weakly known)

Each time the PDA address book opens, its first half second shows a default card with HyperBole Studios' real
Seattle address before the first real contact replaces it. Watch closely.

**Shiloh:** the same.

## Jose Chung's book on the warehouse lock (new)

At the warehouse, when you are there with Skinner, use Jose Chung's book on the door lock instead of the lock-pick:
Skinner reacts as if you had picked it, and a short extra scene is unlocked (we have not checked what it shows). The `action movie` code above makes any item do this.

**Shiloh:** should behave the same (the game's own data drives it), but this one is not tested yet.

## Debug and hotkeys (new)

The game program reacts to Alt keys, most of which the manual does not mention (as far as we know). Nothing disables them in the
released game, but they were found in the program and have not been checked on the original yet.

| keys | what it does | Shiloh |
|---|---|---|
| Shift+Alt+F | frame-rate counter | yes (top-left corner) |
| Shift+Alt+H | outlines of the clickable areas (probably) | yes (the same as F1) |
| Shift+Alt+I | a box with the current chapter and the four hidden scores (Asta, Loss, Paranoia, X) | yes |
| Shift+Alt+J | switches off the "typewriter" effect of on-screen text | no |
| Alt+0 … Alt+9 | volume (Alt+0 mutes, Alt+9 is full) | yes |
| Alt+A / M / R / T / V | switch Artificial Intuition / Subliminal Messages / Rollover Text / Transitions / High Quality Video on or off | yes |
| Alt+P | pause; Alt+P again resumes | yes (any key or click also resumes) |
| Alt+C / S / X | captions / the save screen / exit | yes (Alt+X does nothing in the browser version) |

Browsers keep some Alt combinations for themselves, so a few of these may not reach the browser version.

## Subliminal Messages (new details)

The "Subliminal Messages" option (on by default) really does flash words: every few seconds, outside the movies, the
game draws a single dark-grey word or short phrase in the middle of the screen for a few milliseconds, which is why
almost nobody has ever seen one. Which set of words it uses follows the highest of the hidden scores, so the score
codes above change them. This one was confirmed by running the original game.

**Shiloh:** not yet.

## Cut content

The game data also holds about sixty media files that nothing in the game ever plays: logo movies the intro skips,
cut lines of dialogue (among them a hint line for the riddle at the railyard), unused cursor animations, a piece of
music, a few sounds and some e-mail replies that cannot be triggered. A few cutscenes also have gun targets that
were authored but do nothing when clicked. Shiloh does not play any of these either.

---

Found another one? Open an issue.
