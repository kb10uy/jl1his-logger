try {
    const theme = localStorage.getItem('jl1his-logger.theme');
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
} catch {}
