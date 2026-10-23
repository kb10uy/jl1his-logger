<script lang="ts">
    import { goto } from '$app/navigation';
    import { page } from '$app/state';

    goto(`/logbooks/${page.params.logbook}/log`, { replace: true });
</script>
