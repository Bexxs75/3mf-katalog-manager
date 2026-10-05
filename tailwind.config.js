/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  darkMode: ['attribute', '[data-app="dark"]'],
  theme: {
    extend: {
      fontFamily: { sans: ['Barlow', 'Helvetica', 'sans-serif'] },
      fontSize: {
        caption: 'var(--fs-caption)',
        small: 'var(--fs-small)',
        body: 'var(--fs-body)',
        title: 'var(--fs-title)',
        heading: 'var(--fs-heading)',
      },
    },
  },
  plugins: [],
};
