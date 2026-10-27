<script lang="ts">
    import { getVersion } from '@tauri-apps/api/app';

    const version = getVersion();
</script>

<main>
    <h1>JL1HIS Logger</h1>
    {#await version then value}
        <p>{value}</p>
    {/await}
</main>
