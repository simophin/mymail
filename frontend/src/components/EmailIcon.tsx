import { createEffect, createMemo, createSignal, Show } from "solid-js";
import gravatarUrl from "gravatar-url";

const sizeMap = {
    sm: 16,
    md: 24,
    lg: 40,
}

export default function EmailIcon(props: {
    address: string,
    name?: string | null,
    class?: string,
    size: "sm" | "md" | "lg"
}) {
    const url = createMemo(() => {
        let size = sizeMap[props.size] ?? 24;
        return {
            "url": gravatarUrl(props.address, {
                size: size,
                default: "404",
            }),
            size,
        }
    });

    const initial = createMemo(() => {
        let trimmed = (props.name ?? props.address).trim();
        if (trimmed.length > 0) {
            return trimmed.charAt(0).toUpperCase();
        }
    });

    const [loadState, setLoadState] = createSignal<"loading" | "loaded" | "error">("loading");

    createEffect(() => {
        if (url()) setLoadState("loading");
    });

    return <div class="relative rounded-full">
        <Show when={loadState() != "loaded" && initial()}>
            <div class={`absolute top-0 left-0 flex items-center justify-center bg-gray-300 text-white rounded-full`}
                style={{ width: `${url().size}px`, height: `${url().size}px` }}>
                {initial()}
            </div>
        </Show>

        <img class={`${props.class} ${loadState() === "error" ? 'invisible' : ''}`}
            src={url().url}
            width={url().size}
            height={url().size}
            loading="lazy"
            onLoad={() => setLoadState("loaded")}
            onError={() => setLoadState("error")} />
    </div>
}