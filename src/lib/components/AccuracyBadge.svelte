<script lang="ts">
  import type { Accuracy } from '../types';

  let { accuracy }: { accuracy: Accuracy } = $props();

  const labels: Record<Accuracy, string> = {
    reported_exact: 'Reported',
    derived_exact: 'Derived',
    estimated: 'Estimated',
    unavailable: 'Unavailable',
  };
</script>

<span
  class:estimated={accuracy === 'estimated'}
  class:unavailable={accuracy === 'unavailable'}
  class="badge"
>
  <span class="badge-dot" aria-hidden="true"></span>{labels[accuracy]}
</span>
