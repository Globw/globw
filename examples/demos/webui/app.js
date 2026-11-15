// The page asks the Globw program for each roll: GET /api/roll?sides=N
// answers JSON like {"sides": 20, "value": 13, "rolls": 4}.
let sides = 6;

document.querySelectorAll(".sides button").forEach((b) => {
  b.addEventListener("click", () => {
    sides = Number(b.dataset.sides);
    document.querySelectorAll(".sides button").forEach((o) => o.classList.toggle("on", o === b));
  });
});

document.getElementById("roll").addEventListener("click", async () => {
  const reply = await fetch("/api/roll?sides=" + sides);
  const roll = await reply.json();
  document.getElementById("value").textContent = roll.value;
  const item = document.createElement("li");
  item.textContent = "d" + roll.sides + " → " + roll.value + "  (#" + roll.rolls + ")";
  document.getElementById("history").prepend(item);
});
