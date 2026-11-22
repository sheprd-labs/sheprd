# The header over the turns a conversation brought when it was continued in
# this chat, such as a terminal agent's. It opens and folds them.
chat-earlier-show = { $count ->
    [one] Show { $count } earlier turn
   *[other] Show { $count } earlier turns
}
chat-earlier-hide = { $count ->
    [one] Hide { $count } earlier turn
   *[other] Hide { $count } earlier turns
}
