<script lang="ts">
  // Symmetric envelope from normalized peaks (0..1), drawn like the prototype (viewBox 200×40).
  // `gain` is the vertical waveform zoom (the Edit page).
  let { peaks, fill, gain = 1, class: cls = '' }: { peaks: number[]; fill: string; gain?: number; class?: string } = $props();
  const path = $derived.by(() => {
    const n = peaks.length;
    if (n === 0) return 'M0 20 L200 20 Z';
    let top = '';
    const bot: string[] = [];
    for (let i = 0; i < n; i++) {
      const x = (i / Math.max(1, n - 1)) * 200;
      const h = Math.max(0.4, Math.min(1, peaks[i] * gain) * 18);
      top += `${i ? 'L' : 'M'}${x.toFixed(1)} ${(20 - h).toFixed(1)}`;
      bot.push(`L${x.toFixed(1)} ${(20 + h).toFixed(1)}`);
    }
    return top + bot.reverse().join('') + 'Z';
  });
</script>

<svg class="wv {cls}" viewBox="0 0 200 40" preserveAspectRatio="none" aria-hidden="true">
  <path d={path} style:fill={fill} />
</svg>

<style>
  .wv {
    position: absolute;
    left: 0;
    top: 0;
    width: 100%;
    height: 100%;
  }
</style>
