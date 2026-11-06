<script lang="ts">
  // The app's mark: three takes in a stack, the top one with its lines and waveform.
  let { size = 34 }: { size?: number } = $props();
</script>

<svg viewBox="0 0 100 100" width={size} height={size} aria-hidden="true">
  <rect x="34" y="27" width="46" height="46" rx="10" fill="rgb(var(--accent-rgb) / 0.3)" />
  <rect x="27" y="24.5" width="46" height="51" rx="10.5" fill="rgb(var(--accent-rgb) / 0.55)" />
  <rect x="20" y="22" width="48" height="56" rx="11" fill="var(--accent)" />
  <path d="M28 42 H38 M28 52 H36 M28 62 H39" stroke="var(--on-accent)" stroke-width="4.5" stroke-linecap="round" fill="none" opacity="0.55" />
  <path d="M49 40V44M53 38V46M57 39V45M61 40.5V43.5M49 48.5V55.5M53 46.5V57.5M57 49.5V54.5M61 48V56M49 60.5V63.5M53 58.5V65.5M57 57V67M61 59.5V64.5" stroke="var(--on-accent)" stroke-width="2.6" stroke-linecap="round" fill="none" />
  <path d="M44 31V70" stroke="var(--on-accent)" stroke-width="3.4" stroke-linecap="round" fill="none" />
  <circle cx="44" cy="31" r="5" fill="var(--on-accent)" />
</svg>

<style>
  svg {
    flex: none;
    overflow: visible;
  }
</style>
