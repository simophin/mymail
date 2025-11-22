import {defineConfig} from 'vite';
import solidPlugin from 'vite-plugin-solid';
import devtools from 'solid-devtools/vite';
import tailwindcss from "@tailwindcss/vite";
import solidSvg from "vite-plugin-solid-svg";

export default defineConfig({
    plugins: [devtools(), solidPlugin(), solidSvg(), tailwindcss()],
    server: {
        port: 3000,
    },
    build: {
        target: 'esnext',
    },
});
