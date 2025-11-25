import {isThisWeek, isToday, isYesterday} from "date-fns";

const shortTimeFormatter = new Intl.DateTimeFormat(undefined, {
    timeStyle: "short"
});

const shortDateFormatter = new Intl.DateTimeFormat(undefined, {
    dateStyle: "short"
});

const shortWeekdayFormatter = new Intl.DateTimeFormat(undefined, {
    weekday: "short"
});

export function formatShortDateTime(date: Date): string {
    const now = new Date();
    if (isToday(date)) {
        return shortTimeFormatter.format(date);
    } else if (isYesterday(date)) {
        return "Yesterday";
    } else if (isThisWeek(date)) {
        return shortWeekdayFormatter.format(date);
    } else {
        return shortDateFormatter.format(date);
    }
}