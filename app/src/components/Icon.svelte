<script lang="ts">
  // The Round 2 icon set (stroke icons on a 24 grid, as drawn in the design files).
  export type IconName =
    | 'mic' | 'speaker' | 'sparkle' | 'chevron' | 'chevron-left' | 'chevron-right' | 'play' | 'check' | 'x' | 'download'
    | 'plus' | 'pointer' | 'split' | 'fade' | 'gain' | 'marquee' | 'start' | 'cycle' | 'inspector' | 'mixer' | 'back' | 'grip' | 'stop'
    | 'lock' | 'unlock';
  let { name, size = 15, stroke = 2 }: { name: IconName; size?: number; stroke?: number } = $props();
</script>

{#if name === 'grip'}
  <svg width="10" height="14" viewBox="0 0 10 14" fill="currentColor" aria-hidden="true"><circle cx="3" cy="3" r="1.2" /><circle cx="7" cy="3" r="1.2" /><circle cx="3" cy="7" r="1.2" /><circle cx="7" cy="7" r="1.2" /><circle cx="3" cy="11" r="1.2" /><circle cx="7" cy="11" r="1.2" /></svg>
{:else if name === 'play'}
  <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5v14l11-7z" fill="currentColor" /></svg>
{:else if name === 'stop'}
  <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true"><rect x="6" y="6" width="12" height="12" rx="2" fill="currentColor" /></svg>
{:else if name === 'pointer'}
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M5 3l14 8-6 2-2 6z" /></svg>
{:else if name === 'start'}
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><rect x="5" y="5" width="2.5" height="14" rx="1" /><path d="M19 5v14L9 12z" /></svg>
{:else}
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width={stroke} stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    {#if name === 'mic'}<rect x="9" y="3" width="6" height="11" rx="3" /><path d="M5 11a7 7 0 0 0 14 0" /><line x1="12" y1="18" x2="12" y2="21" />
    {:else if name === 'speaker'}<path d="M4 10v4h4l5 4V6L8 10z" /><path d="M17 9a4 4 0 0 1 0 6" />
    {:else if name === 'sparkle'}<path d="M12 3l2 7 7 2-7 2-2 7-2-7-7-2 7-2z" />
    {:else if name === 'chevron'}<polyline points="6 9 12 15 18 9" />
    {:else if name === 'chevron-left'}<polyline points="15 6 9 12 15 18" />
    {:else if name === 'chevron-right'}<polyline points="9 6 15 12 9 18" />
    {:else if name === 'check'}<polyline points="5 12 10 17 19 7" />
    {:else if name === 'x'}<line x1="6" y1="6" x2="18" y2="18" /><line x1="18" y1="6" x2="6" y2="18" />
    {:else if name === 'download'}<path d="M12 4v11" /><polyline points="7 10 12 15 17 10" /><path d="M5 19h14" />
    {:else if name === 'plus'}<line x1="12" y1="5" x2="12" y2="19" /><line x1="5" y1="12" x2="19" y2="12" />
    {:else if name === 'split'}<circle cx="6" cy="6" r="3" /><circle cx="6" cy="18" r="3" /><line x1="20" y1="4" x2="8.1" y2="15.9" /><line x1="14.5" y1="14.5" x2="20" y2="20" />
    {:else if name === 'fade'}<path d="M3 20 Q10 4 21 4" />
    {:else if name === 'gain'}<polyline points="3 16 9 10 14 13 21 6" />
    {:else if name === 'lock'}<rect x="5" y="11" width="14" height="9" rx="2" /><path d="M8 11V8a4 4 0 0 1 8 0v3" />
    {:else if name === 'unlock'}<rect x="5" y="11" width="14" height="9" rx="2" /><path d="M8 11V8a4 4 0 0 1 7.5-2" />
    {:else if name === 'marquee'}<rect x="4" y="5" width="16" height="14" rx="2" stroke-dasharray="3 3" />
    {:else if name === 'cycle'}<path d="M17 2l4 4-4 4" /><path d="M3 11v-1a4 4 0 0 1 4-4h14" /><path d="M7 22l-4-4 4-4" /><path d="M21 13v1a4 4 0 0 1-4 4H3" />
    {:else if name === 'inspector'}<rect x="3" y="4" width="18" height="16" rx="3" /><line x1="9" y1="4" x2="9" y2="20" />
    {:else if name === 'mixer'}<line x1="6" y1="4" x2="6" y2="20" /><line x1="12" y1="4" x2="12" y2="20" /><line x1="18" y1="4" x2="18" y2="20" /><rect x="4" y="13" width="4" height="3" rx="1" fill="currentColor" /><rect x="10" y="7" width="4" height="3" rx="1" fill="currentColor" /><rect x="16" y="11" width="4" height="3" rx="1" fill="currentColor" />
    {:else if name === 'back'}<polyline points="15 6 9 12 15 18" />
    {/if}
  </svg>
{/if}
