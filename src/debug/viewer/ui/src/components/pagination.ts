import van from "vanjs-core";

const { div, input, button } = van.tags;

export const Pagination = (page: number, setPageNum: (num: number) => void) => {
  return div(
    { class: "flex flex-row" },
    button(
      {
        class:
          "dark:hover:bg-gray-800 hover:bg-gray-200 px-4 py-1 disabled:opacity-5",
        onclick: () => setPageNum(page > 0 ? page - 1 : page),
        disabled: page === 0,
      },
      "<",
    ),
    input({
      class: "px-4 py-1",
      value: page,
      onchange: (e) => setPageNum(Number(e.target.value)),
      type: "number",
    }),
    button(
      {
        class:
          "dark:hover:bg-gray-800 hover:bg-gray-200 px-4 py-1 disabled:opacity-5",
        onclick: () => setPageNum(page + 1),
      },
      ">",
    ),
  );
};
