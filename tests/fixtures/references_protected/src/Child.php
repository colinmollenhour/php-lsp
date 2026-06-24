<?php

namespace App;

class Child extends Base
{
    public function start(): void
    {
        $this->boot();
    }
}
