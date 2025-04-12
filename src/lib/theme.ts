import { writable } from "svelte/store";

type Theme = 'light' | 'dark';
const isValidTheme = (value: string | null): value is Theme =>
  value === 'light' || value === 'dark';

const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
const localStorageTheme = localStorage.getItem('theme');

const initialTheme: Theme = isValidTheme(localStorageTheme)
  ? localStorageTheme
  : prefersDark
    ? 'dark'
    : 'light';
export const theme = writable<Theme>(initialTheme);

theme.subscribe((v) => {
  console.log(v)
  document.documentElement.classList.toggle('dark', v === 'dark');
  localStorage.setItem('theme', v);
})