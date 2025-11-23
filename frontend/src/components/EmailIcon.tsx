import {createMemo} from "solid-js";
import gravatarUrl from "gravatar-url";

const sizeMap = {
    sm: 16,
    md: 24,
    lg: 40,
}

export default function EmailIcon(props: {
    address: string,
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

    return <img class={props.class}
                src={url().url}
                width={url().size}
                height={url().size}
                alt="X"/>;
}