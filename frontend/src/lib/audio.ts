/**
 * Synthesized board sounds: no audio assets, just WebAudio.
 *
 * A move is a wooden "tock" (noise click + pitched thud), a capture lands a
 * second, lower knock, autoplayed moves are softer, and the game ends on a
 * small arpeggio (rising for mate, a falling sigh for a draw).
 */

let ctx: AudioContext | null = null;

function ac(): AudioContext | null {
	if (typeof window === 'undefined') return null;
	ctx ??= new AudioContext();
	if (ctx.state === 'suspended') void ctx.resume();
	return ctx;
}

function thud(c: AudioContext, t: number, freq: number, peak: number, dur: number) {
	const o = c.createOscillator();
	const g = c.createGain();
	o.type = 'sine';
	o.frequency.setValueAtTime(freq, t);
	o.frequency.exponentialRampToValueAtTime(Math.max(40, freq * 0.45), t + dur);
	g.gain.setValueAtTime(peak, t);
	g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
	o.connect(g).connect(c.destination);
	o.start(t);
	o.stop(t + dur + 0.02);
}

function click(c: AudioContext, t: number, peak: number) {
	const len = Math.floor(c.sampleRate * 0.03);
	const buf = c.createBuffer(1, len, c.sampleRate);
	const d = buf.getChannelData(0);
	for (let i = 0; i < len; i++) d[i] = (Math.random() * 2 - 1) * (1 - i / len);
	const src = c.createBufferSource();
	src.buffer = buf;
	const f = c.createBiquadFilter();
	f.type = 'bandpass';
	f.frequency.value = 2400;
	f.Q.value = 1.2;
	const g = c.createGain();
	g.gain.value = peak;
	src.connect(f).connect(g).connect(c.destination);
	src.start(t);
}

export function moveSound(capture = false) {
	const c = ac();
	if (!c) return;
	const t = c.currentTime + 0.01;
	click(c, t, 0.22);
	thud(c, t, capture ? 140 : 190, capture ? 0.45 : 0.32, 0.1);
	if (capture) thud(c, t + 0.055, 110, 0.38, 0.12);
}

/** Softer tock for moves the archive plays by itself. */
export function forcedSound(capture = false) {
	const c = ac();
	if (!c) return;
	const t = c.currentTime + 0.01;
	click(c, t, 0.1);
	thud(c, t, capture ? 150 : 210, 0.16, 0.08);
}

export function endSound(mate: boolean) {
	const c = ac();
	if (!c) return;
	const t = c.currentTime + 0.02;
	const notes = mate ? [392, 523.25, 659.25, 783.99] : [440, 415.3, 392];
	notes.forEach((f, i) => {
		const o = c.createOscillator();
		const g = c.createGain();
		o.type = 'triangle';
		o.frequency.value = f;
		const st = t + i * 0.12;
		g.gain.setValueAtTime(0.0001, st);
		g.gain.exponentialRampToValueAtTime(0.16, st + 0.02);
		g.gain.exponentialRampToValueAtTime(0.0001, st + 0.7);
		o.connect(g).connect(c.destination);
		o.start(st);
		o.stop(st + 0.75);
	});
}
