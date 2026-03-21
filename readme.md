# Radio

> caveats:
> 1. turn up autoplay if no sound, though technically mainstream browsers (at least pc) dont block webaudio that aggressively
> 2. spectrogram is disabled on this instance, and firefox mobile spectrogram wont work even if enabled, probably bad webgpu support yet
> 3. use a proxy if lags, even it's proxied through cloudflare (disclaimer: definitely not my server lags)
> 4. if you found spectrogram lagging, i know spectrogram lags but i gave up, details below
> 5. THIS IS NOT EXHIBITIONISM AS I EXPECT YOU TO PUBLICLY DEFINE

 [Instance](https://radio.997779.best) of Personal Radio over IP (RoIP) [well i just invented that] full stack

more like a WebSDR but linked to your laptop, and you can route any audio feed simply via pipewire

basically musical exhibitionist's favorite

### How it works

server side gives you a rust backend, create an audio node that connects to an audio device where you want to reroute your audio feed to, this 'extra' layer is for consistency and automation safety

inside server it fft'd to get realtime spectrogram (where it gets sdr-ish), uses pw internal resampler to downsample (mind your bandwidth) (and pw internal resampler is PEAK compared to my linear interpolation attempt) and that will be the output audio feed

the true spice here is the transport

like normally when you try to stream something, you just think existing protocols, ll-hls, rtmp, rtsp, webrtc-whip, nada nada

but they all got *some flaws

ll-hls (at least mediamtx implementation) lags constantly(average latency around 2000ms) and got gaps between segments, and its catch-up logic is stupid enough to just speed up/down even without pitch shift, so yeah unrecommended unless you wanna ride your audio on roller coaster

rtmp/s are fucked on web, rtsp/rtp ditto

webrtc is the closest and it is my top candidate until decided to build my own, but first it is built on udp, so say goodbye to reverse proxies and ready to leak your ip(v6 which is fatal), and rtc connections on web arent quite customizable, so hell to tamper the audio routing if running visualization on web, rather to say the autoplay annoyances

so my solution was... stream using websocket!! tada!

(stupid af) ahem, basically the server compresses audio to opus (praise thy opus god!!!!) and packs several samples in frames and packs several frames in ws messages(dependent on your in/out sample rate ratio, say on your pc its 48k and web 12k, then you got 4 frames/message)

sounds stupidly simple, but actually it's beneficial: first its tcp so no ip leak bollocks, and pretty much every reverse proxy supports ws, so you get free ipv4/6 global reach, and its realtime(like just 300ms? basic buffer latency plus true internet latency), yay!

and server also ships spectrogram data using ws, but the obstinate problem is **fucking rendering**

yeah you might think just slap a buffer on and roll the graph, but *technically very wrong*

since it's multiple frames packed in a message and you dont know when will next message arrive, so a expected gap and catch-up logic is necessary and inherently ugly, and regular buffers wont work here since you still dont know when to draw the next frame, unlike a regular SDR flow where you just pop the frames and draw(due to frame packing originated from high update rate), rather to say subpixel blending and drawing, gpu textures and stuff...

so you see spectrogram component in App.svelte is commented out, literally fucking hell, and i give up, probably looking for someone able to make this elegantly running to issue a PR?

### specifications

see example toml for server side arguments

see nginx.conf for example reverse proxy config

see pw confs for creating the audio sink and route audio through

### !

yeaaaaa finally ditching vue!!!!

but more like ditching agents who wrote me vue2 boilerplates

and honestly ts+svelte isnt that ugly... for now..?