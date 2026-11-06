<script lang="ts">
  // While a project loads: its cover, its title and how it's opening. It goes the moment the
  // load is done; nothing here waits (D150).
  import { fade } from 'svelte/transition';
  import { app } from '../lib/app.svelte';

  const o = $derived(app.opening);
</script>

{#if o}
  <div class="open" role="status" data-testid="opening" transition:fade={{ duration: 140 }}>
    <div class="bg" style="background: {o.image}; background-size: cover; background-position: center"></div>
    <div class="dim"></div>
    <div class="mid">
      <span class="mode" class:rv={o.mode === 'review'}>{o.mode === 'review' ? 'Review · read-only' : 'Edit'}</span>
      <div class="t ellip">{o.title}</div>
      <div class="k">{o.kicker}</div>
      <div class="bar"><span></span></div>
    </div>
  </div>
{/if}

<style>
  .open {
    position: absolute;
    inset: 0;
    z-index: 70;
    overflow: hidden;
    background: var(--stage-bg);
    color: var(--text-strong);
  }
  .bg {
    position: absolute;
    inset: -40px;
    filter: blur(30px);
  }
  .dim {
    position: absolute;
    inset: 0;
    background: rgb(var(--night-rgb) / 0.6);
  }
  .mid {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 0 48px;
  }
  .mode {
    height: 26px;
    padding: 0 12px;
    border-radius: var(--radius-pill);
    display: inline-flex;
    align-items: center;
    font-size: 12px;
    font-weight: 700;
    background: var(--button-light);
    color: var(--text-on-light);
  }
  .mode.rv {
    background: var(--accent);
    color: var(--on-accent);
  }
  .t {
    max-width: 100%;
    margin-top: 8px;
    font-size: 40px;
    font-weight: 800;
    letter-spacing: -0.02em;
  }
  .k {
    font-size: 14px;
    opacity: 0.66;
  }
  /* how long a load takes isn't known, so this says it's working, not how far along */
  .bar {
    width: 220px;
    height: 3px;
    margin-top: 18px;
    border-radius: 2px;
    overflow: hidden;
    background: rgb(var(--veil-rgb) / 0.15);
  }
  .bar span {
    display: block;
    width: 40%;
    height: 100%;
    border-radius: 2px;
    background: var(--accent);
    animation: slide 1.1s ease-in-out infinite alternate;
  }
  @keyframes slide {
    from {
      transform: translateX(-20%);
    }
    to {
      transform: translateX(170%);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .bar span {
      animation: none;
      width: 100%;
    }
  }
</style>
