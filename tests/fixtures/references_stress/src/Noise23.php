<?php

namespace App;

class Noise23
{
    private function compute(): int
    {
        return 23;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
