import {onMount, untrack} from "solid-js";


export function AdjustableHorizontalDivider(props: {
    thickness?: number,
    size: number,
    onSizeChange: (newSize: number) => void,
    onDragStarted?: () => void,
    onDragEnded?: () => void,
    class?: string,
}) {
    return <div
        class={`${props.class} h-full w-3 bg-base-200 cursor-w-resize ml-1 mr-1`}
        onMouseDown={(e) => {
            e.preventDefault();

            const startX = e.clientX;
            const startSize = untrack(() => props.size);
            props.onDragStarted?.();

            function onMouseMove(e: MouseEvent) {
                props.onSizeChange(startSize + (e.clientX - startX));
            }

            function onMouseUp(e: MouseEvent) {
                window.removeEventListener('mousemove', onMouseMove);
                window.removeEventListener('mouseup', onMouseUp);

                props.onDragEnded?.();
            }

            window.addEventListener('mousemove', onMouseMove);
            window.addEventListener('mouseup', onMouseUp);
        }}/>
}