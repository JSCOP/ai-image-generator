<script lang="ts">
  // The one confirm dialog of the app, opened by state.ask().
  import { answer, confirmState } from '../lib/state.svelte';
  import Button from './Button.svelte';
  import Dialog from './Dialog.svelte';
</script>

<Dialog open={confirmState.open} title={confirmState.options.title} onclose={() => answer(false)}>
  <p>{confirmState.options.message}</p>
  {#snippet actions()}
    <Button onclick={() => answer(false)}>{confirmState.options.cancelText ?? '취소'}</Button>
    <Button variant={confirmState.options.danger ? 'danger' : 'primary'} onclick={() => answer(true)}>
      {confirmState.options.confirmText ?? '확인'}
    </Button>
  {/snippet}
</Dialog>

<style>
  p {
    margin: 0;
    white-space: pre-line;
  }
</style>
