<script lang="ts">
  import { brand } from './lib/brand';
</script>

<main class="stage">
  <h1>{brand.displayName}</h1>
</main>

<style>
  .stage {
    min-height: 100vh;
    background: var(--stage-bg);
    color: var(--text-1);
    font-family: var(--font-ui);
  }
</style>
