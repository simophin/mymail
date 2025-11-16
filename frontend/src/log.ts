import pino from "pino";

export const log = pino({
    level: "debug",
    browser: {
        serialize: true
    }
});